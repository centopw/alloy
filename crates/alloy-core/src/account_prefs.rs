//! Account-synced preferences. The UI's synced settings are one JSON object kept in global
//! account data (`chat.alloy.prefs`), so every device signed in to the account shares them. The
//! engine does not interpret the document beyond two checks: it must be a JSON object of at most
//! 64 KiB, and it must carry an integer `updated_at` (milliseconds) that orders writes.

use crate::{AlloyClient, AlloyError, AlloyResult};
use matrix_sdk::ruma::events::{
    AnyGlobalAccountDataEvent, AnyGlobalAccountDataEventContent, GlobalAccountDataEventType,
};
use matrix_sdk::ruma::serde::Raw;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Global account-data type holding the synced preferences document.
const ACCOUNT_PREFS_EVENT_TYPE: &str = "chat.alloy.prefs";

/// Largest accepted preferences document, in bytes.
pub const MAX_ACCOUNT_PREFS_BYTES: usize = 64 * 1024;

/// Validates a preferences document and returns it parsed, with its `updated_at`.
fn checked_prefs(json: &str) -> AlloyResult<(serde_json::Value, u64)> {
    if json.len() > MAX_ACCOUNT_PREFS_BYTES {
        return Err(AlloyError::InvalidPrefs(format!(
            "preferences exceed {MAX_ACCOUNT_PREFS_BYTES} bytes"
        )));
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| AlloyError::InvalidPrefs(format!("preferences are not JSON: {e}")))?;
    let updated_at = value
        .as_object()
        .ok_or_else(|| AlloyError::InvalidPrefs("preferences must be a JSON object".into()))?
        .get("updated_at")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| AlloyError::InvalidPrefs("preferences need an integer updated_at".into()))?;
    Ok((value, updated_at))
}

impl AlloyClient {
    /// The account's synced preferences document as stored, or `None` when none was saved.
    pub async fn get_account_prefs(&self) -> AlloyResult<Option<String>> {
        let stored = self
            .client
            .account()
            .fetch_account_data(GlobalAccountDataEventType::from(ACCOUNT_PREFS_EVENT_TYPE))
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(stored.map(|raw| raw.json().get().to_owned()))
    }

    /// Replaces the synced preferences document after validating it.
    pub async fn set_account_prefs(&self, json: &str) -> AlloyResult<()> {
        let (value, updated_at) = checked_prefs(json)?;
        let content = Raw::new(&value)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
            .cast_unchecked::<AnyGlobalAccountDataEventContent>();
        self.client
            .account()
            .set_account_data_raw(
                GlobalAccountDataEventType::from(ACCOUNT_PREFS_EVENT_TYPE),
                content,
            )
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        self.prefs_updated_at.store(updated_at, Ordering::SeqCst);
        Ok(())
    }

    /// Reports each synced preferences document that another device wrote. This client's own
    /// writes, and the server echoes of them, are not reported: a change is reported only when
    /// its `updated_at` differs from the newest one this client wrote or reported. Handlers
    /// share that state, so registering this twice still reports each change once.
    pub fn watch_account_prefs(&self, on_change: Arc<dyn Fn(String) + Send + Sync>) {
        let seen = self.prefs_updated_at.clone();
        self.client
            .add_event_handler(move |ev: Raw<AnyGlobalAccountDataEvent>| {
                let (on_change, seen) = (on_change.clone(), seen.clone());
                async move {
                    let is_prefs = ev.get_field::<String>("type").ok().flatten().as_deref()
                        == Some(ACCOUNT_PREFS_EVENT_TYPE);
                    if !is_prefs {
                        return;
                    }
                    let Ok(Some(content)) = ev.get_field::<serde_json::Value>("content") else {
                        return;
                    };
                    let Ok(json) = serde_json::to_string(&content) else {
                        return;
                    };
                    let Ok((_, updated_at)) = checked_prefs(&json) else {
                        return;
                    };
                    if seen.swap(updated_at, Ordering::SeqCst) != updated_at {
                        on_change(json);
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_object_with_integer_updated_at() {
        let (value, updated_at) = checked_prefs(r#"{"updated_at":42,"timeFormat":"24h"}"#).unwrap();
        assert_eq!(updated_at, 42);
        assert_eq!(value["timeFormat"], "24h");
    }

    #[test]
    fn rejects_documents_that_are_not_a_valid_prefs_object() {
        for json in [
            "not json",
            "[1,2]",
            r#""text""#,
            r#"{"timeFormat":"24h"}"#,
            r#"{"updated_at":"42"}"#,
            r#"{"updated_at":-1}"#,
            r#"{"updated_at":1.5}"#,
        ] {
            assert!(
                matches!(checked_prefs(json), Err(AlloyError::InvalidPrefs(_))),
                "accepted {json}"
            );
        }
    }

    #[test]
    fn rejects_documents_over_the_size_limit() {
        let json =
            serde_json::json!({ "updated_at": 1, "pad": "x".repeat(MAX_ACCOUNT_PREFS_BYTES) })
                .to_string();
        assert!(matches!(
            checked_prefs(&json),
            Err(AlloyError::InvalidPrefs(_))
        ));
    }
}
