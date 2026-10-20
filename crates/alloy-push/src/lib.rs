//! # alloy-push
//!
//! Background push notification client for Alloy on Android.
//!
//! Handles parsing of UnifiedPush/Matrix payloads, fetching notification content with
//! `matrix-sdk-ui::NotificationClient` in multi-process mode, and managing pushers
//! with the homeserver.

use std::path::Path;
use std::time::Duration;

use matrix_sdk_ui::notification_client::{
    NotificationClient, NotificationEvent, NotificationItem, NotificationProcessSetup,
    NotificationStatus,
};
use ruma::api::client::push::{PusherIds, PusherInit, PusherKind};
use ruma::events::room::message::MessageType;
use ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent};
use ruma::push::{HttpPusherData, PushFormat};
use ruma::{EventId, RoomId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use alloy_core::accounts::{restore_account, restore_active_account};

/// Total timeout for fetching a push notification before falling back to generic.
pub const NOTIFICATION_FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Error type for push notification operations.
#[derive(Debug, Error)]
pub enum PushError {
    #[error("Invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid push payload")]
    InvalidPayload,
    #[error("Account error: {0}")]
    Account(#[from] alloy_core::AlloyError),
    #[error("Matrix SDK error: {0}")]
    MatrixSdk(#[from] matrix_sdk::Error),
    #[error("Invalid Matrix ID: {0}")]
    MatrixId(#[from] ruma::IdParseError),
    #[error("Timeout fetching notification")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// Parsed push notification reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushRef {
    pub room_id: String,
    pub event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread: Option<u64>,
}

/// Parse a push payload JSON string into a structured reference.
///
/// Accepts both standard Matrix Push Gateway payloads (nested under `"notification"`)
/// and flattened payloads containing `room_id` and `event_id`.
pub fn parse_push_payload(payload_json: &str) -> Option<PushRef> {
    let value: serde_json::Value = serde_json::from_str(payload_json).ok()?;

    let notif = if let Some(n) = value.get("notification").and_then(|n| n.as_object()) {
        n
    } else {
        value.as_object()?
    };

    let room_id = notif.get("room_id")?.as_str()?.to_string();
    let event_id = notif.get("event_id")?.as_str()?.to_string();

    let unread = notif
        .get("counts")
        .and_then(|c| c.get("unread"))
        .and_then(|u| u.as_u64())
        .or_else(|| notif.get("unread").and_then(|u| u.as_u64()));

    Some(PushRef {
        room_id,
        event_id,
        unread,
    })
}

/// Outcome of mapping a notification for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NotificationOutcome {
    Event {
        title: String,
        body: String,
        room_id: String,
        event_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        room_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sender_name: Option<String>,
        is_dm: bool,
        noisy: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        unread: Option<u64>,
    },
    Generic {
        title: String,
        body: String,
        room_id: String,
        event_id: String,
    },
    /// Nothing to show; `reason` says why (`own_message`, `reaction`, `filtered_out`, `redacted`).
    Skip {
        reason: String,
    },
    Error {
        error: String,
    },
}

impl NotificationOutcome {
    pub fn skip(reason: &str) -> Self {
        NotificationOutcome::Skip {
            reason: reason.to_string(),
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|e| {
            serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            })
        })
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!(r#"{{"status":"error","error":"{}"}}"#, e))
    }
}

enum MessageBodyOutcome {
    Text(String),
    Skip(&'static str),
    Generic,
}

fn extract_body_from_item(item: &NotificationItem) -> MessageBodyOutcome {
    match &item.event {
        NotificationEvent::Timeline(boxed_ev) => match boxed_ev.as_ref() {
            AnySyncTimelineEvent::MessageLike(msg) => match msg {
                AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(m)) => match &m
                    .content
                    .msgtype
                {
                    MessageType::Text(c) => MessageBodyOutcome::Text(c.body.clone()),
                    MessageType::Notice(c) => MessageBodyOutcome::Text(c.body.clone()),
                    MessageType::Emote(c) => MessageBodyOutcome::Text(format!("* {}", c.body)),
                    MessageType::Image(_) => MessageBodyOutcome::Text("Sent a photo".to_string()),
                    MessageType::Video(_) => MessageBodyOutcome::Text("Sent a video".to_string()),
                    MessageType::File(_) => MessageBodyOutcome::Text("Sent a file".to_string()),
                    MessageType::Audio(c) => {
                        if c.voice.is_some() {
                            MessageBodyOutcome::Text("Sent a voice message".to_string())
                        } else {
                            MessageBodyOutcome::Text("Sent an audio message".to_string())
                        }
                    }
                    MessageType::Location(_) => {
                        MessageBodyOutcome::Text("Sent a location".to_string())
                    }
                    _ => MessageBodyOutcome::Text(m.content.body().to_string()),
                },
                AnySyncMessageLikeEvent::Reaction(_) => MessageBodyOutcome::Skip("reaction"),
                AnySyncMessageLikeEvent::RoomEncrypted(_) => MessageBodyOutcome::Generic,
                _ => {
                    let ev_type = msg.event_type().to_string();
                    if ev_type.contains("poll") {
                        MessageBodyOutcome::Text("Started a poll".to_string())
                    } else if ev_type == "m.reaction" {
                        MessageBodyOutcome::Skip("reaction")
                    } else {
                        MessageBodyOutcome::Text("New message".to_string())
                    }
                }
            },
            // Membership, encryption and other state changes are not messages; invites arrive as `Invite`.
            AnySyncTimelineEvent::State(_) => MessageBodyOutcome::Skip("state_event"),
        },
        NotificationEvent::Invite(_) => {
            let room_name = if item.room_computed_display_name.is_empty() {
                "a room"
            } else {
                item.room_computed_display_name.as_str()
            };
            MessageBodyOutcome::Text(format!("Invited you to {room_name}"))
        }
    }
}

/// Map a [`NotificationStatus`] and [`PushRef`] to the final [`NotificationOutcome`].
pub fn map_notification_status(
    status: NotificationStatus,
    push_ref: &PushRef,
    own_user_id: Option<&str>,
) -> NotificationOutcome {
    match status {
        NotificationStatus::Event(item) => {
            // Own messages are skipped
            if own_user_id == Some(item.event.sender().as_str()) {
                return NotificationOutcome::skip("own_message");
            }

            let body = match extract_body_from_item(&item) {
                MessageBodyOutcome::Text(text) => text,
                MessageBodyOutcome::Skip(reason) => return NotificationOutcome::skip(reason),
                MessageBodyOutcome::Generic => {
                    return NotificationOutcome::Generic {
                        title: "Alloy".to_string(),
                        body: "New message".to_string(),
                        room_id: push_ref.room_id.clone(),
                        event_id: push_ref.event_id.clone(),
                    };
                }
            };

            let is_dm = item.room_is_dm || item.is_direct_message_room;
            let sender_display = item
                .sender_display_name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| item.event.sender().as_str());

            let title = if is_dm {
                sender_display.to_string()
            } else {
                let room_display = if item.room_computed_display_name.trim().is_empty() {
                    "Room"
                } else {
                    item.room_computed_display_name.as_str()
                };
                format!("{sender_display} in {room_display}")
            };

            let room_name = if item.room_computed_display_name.is_empty() {
                None
            } else {
                Some(item.room_computed_display_name)
            };

            NotificationOutcome::Event {
                title,
                body,
                room_id: push_ref.room_id.clone(),
                event_id: push_ref.event_id.clone(),
                room_name,
                sender_name: Some(sender_display.to_string()),
                is_dm,
                noisy: item.is_noisy.unwrap_or(true),
                unread: push_ref.unread,
            }
        }
        NotificationStatus::EventFilteredOut => NotificationOutcome::skip("filtered_out"),
        NotificationStatus::EventRedacted => NotificationOutcome::skip("redacted"),
        NotificationStatus::EventNotFound => NotificationOutcome::Generic {
            title: "Alloy".to_string(),
            body: "New message".to_string(),
            room_id: push_ref.room_id.clone(),
            event_id: push_ref.event_id.clone(),
        },
    }
}

/// Fetch notification content given the app's `data_dir` and the raw push JSON payload.
///
/// Implements status rules: `event`, `generic`, `skip`, or `error`.
/// Enforces a 20-second timeout, falling back to `generic` on timeout.
pub async fn fetch_notification(data_dir: &Path, payload_json: &str) -> serde_json::Value {
    let push_ref = match parse_push_payload(payload_json) {
        Some(r) => r,
        None => {
            return NotificationOutcome::Error {
                error: "invalid push payload".to_string(),
            }
            .to_json_value();
        }
    };

    let (client, _entry) = match restore_active_account(data_dir, None).await {
        Ok(res) => res,
        Err(e) => {
            return NotificationOutcome::Error {
                error: format!("failed to restore account: {e}"),
            }
            .to_json_value();
        }
    };

    let room_id = match RoomId::parse(&push_ref.room_id) {
        Ok(r) => r,
        Err(e) => {
            return NotificationOutcome::Error {
                error: format!("invalid room_id: {e}"),
            }
            .to_json_value();
        }
    };

    let event_id = match EventId::parse(&push_ref.event_id) {
        Ok(e) => e,
        Err(e) => {
            return NotificationOutcome::Error {
                error: format!("invalid event_id: {e}"),
            }
            .to_json_value();
        }
    };

    let notif_client = match NotificationClient::new(
        client.inner_client().clone(),
        NotificationProcessSetup::MultipleProcesses,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return NotificationOutcome::Error {
                error: format!("failed to create notification client: {e}"),
            }
            .to_json_value();
        }
    };

    let fetch_fut = notif_client.get_notification(&room_id, &event_id);
    let outcome = match tokio::time::timeout(NOTIFICATION_FETCH_TIMEOUT, fetch_fut).await {
        Ok(Ok(status)) => map_notification_status(status, &push_ref, client.user_id()),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "notification client error, falling back to generic");
            NotificationOutcome::Generic {
                title: "Alloy".to_string(),
                body: "New message".to_string(),
                room_id: push_ref.room_id,
                event_id: push_ref.event_id,
            }
        }
        Err(_) => {
            tracing::warn!("notification fetch timed out, falling back to generic");
            NotificationOutcome::Generic {
                title: "Alloy".to_string(),
                body: "New message".to_string(),
                room_id: push_ref.room_id,
                event_id: push_ref.event_id,
            }
        }
    };

    outcome.to_json_value()
}

/// Pusher registration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PusherConfig {
    pub app_id: String,
    pub pushkey: String,
    #[serde(default)]
    pub gateway_url: String,
    #[serde(default = "default_app_name")]
    pub app_display_name: String,
    #[serde(default = "default_device_name")]
    pub device_display_name: String,
    #[serde(default = "default_lang")]
    pub lang: String,
}

fn default_app_name() -> String {
    "Alloy".to_string()
}

fn default_device_name() -> String {
    "Android Device".to_string()
}

fn default_lang() -> String {
    "en".to_string()
}

/// Register an HTTP pusher with the homeserver for the active account.
pub async fn set_pusher(data_dir: &Path, config_json: &str) -> serde_json::Value {
    match set_pusher_inner(data_dir, config_json).await {
        Ok(()) => serde_json::json!({ "ok": true }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn set_pusher_inner(data_dir: &Path, config_json: &str) -> Result<(), PushError> {
    let config: PusherConfig = serde_json::from_str(config_json)?;
    let (client, _entry) = restore_active_account(data_dir, None).await?;

    let mut http_data = HttpPusherData::new(config.gateway_url);
    http_data.format = Some(PushFormat::EventIdOnly);

    let pusher = PusherInit {
        ids: PusherIds::new(config.pushkey, config.app_id),
        kind: PusherKind::Http(http_data),
        app_display_name: config.app_display_name,
        device_display_name: config.device_display_name,
        profile_tag: None,
        lang: config.lang,
    };

    client
        .inner_client()
        .pusher()
        .set(pusher.into(), false)
        .await
        .map_err(PushError::MatrixSdk)?;

    Ok(())
}

/// Remove a pusher from the homeserver for the active account.
pub async fn remove_pusher(data_dir: &Path, config_json: &str) -> serde_json::Value {
    match remove_pusher_inner(data_dir, config_json).await {
        Ok(()) => serde_json::json!({ "ok": true }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn remove_pusher_inner(data_dir: &Path, config_json: &str) -> Result<(), PushError> {
    let config: PusherConfig = serde_json::from_str(config_json)?;
    let (client, _entry) = restore_active_account(data_dir, None).await?;

    let ids = PusherIds::new(config.pushkey, config.app_id);
    client
        .inner_client()
        .pusher()
        .delete(ids)
        .await
        .map_err(PushError::MatrixSdk)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_push_payload_nested() {
        let json = r#"{
            "notification": {
                "event_id": "$ev123:example.org",
                "room_id": "!room456:example.org",
                "counts": {
                    "unread": 5
                },
                "prio": "high"
            }
        }"#;

        let res = parse_push_payload(json).expect("should parse");
        assert_eq!(res.event_id, "$ev123:example.org");
        assert_eq!(res.room_id, "!room456:example.org");
        assert_eq!(res.unread, Some(5));
    }

    #[test]
    fn parse_push_payload_flat() {
        let json = r#"{
            "event_id": "$ev789:example.org",
            "room_id": "!room012:example.org",
            "unread": 2
        }"#;

        let res = parse_push_payload(json).expect("should parse");
        assert_eq!(res.event_id, "$ev789:example.org");
        assert_eq!(res.room_id, "!room012:example.org");
        assert_eq!(res.unread, Some(2));
    }

    #[test]
    fn parse_push_payload_missing_required() {
        assert!(parse_push_payload("{}").is_none());
        assert!(parse_push_payload(r#"{"event_id": "$1"}"#).is_none());
        assert!(parse_push_payload(r#"{"room_id": "!1"}"#).is_none());
        assert!(parse_push_payload("not json").is_none());
    }

    #[test]
    fn json_shapes_status_event() {
        let outcome = NotificationOutcome::Event {
            title: "Alice in General".to_string(),
            body: "Hello world!".to_string(),
            room_id: "!room:example.org".to_string(),
            event_id: "$ev:example.org".to_string(),
            room_name: Some("General".to_string()),
            sender_name: Some("Alice".to_string()),
            is_dm: false,
            noisy: true,
            unread: Some(3),
        };

        let val = outcome.to_json_value();
        assert_eq!(val["status"], "event");
        assert_eq!(val["title"], "Alice in General");
        assert_eq!(val["body"], "Hello world!");
        assert_eq!(val["room_id"], "!room:example.org");
        assert_eq!(val["event_id"], "$ev:example.org");
        assert_eq!(val["room_name"], "General");
        assert_eq!(val["sender_name"], "Alice");
        assert_eq!(val["is_dm"], false);
        assert_eq!(val["noisy"], true);
        assert_eq!(val["unread"], 3);
    }

    #[test]
    fn json_shapes_status_generic() {
        let outcome = NotificationOutcome::Generic {
            title: "Alloy".to_string(),
            body: "New message".to_string(),
            room_id: "!room:example.org".to_string(),
            event_id: "$ev:example.org".to_string(),
        };

        let val = outcome.to_json_value();
        assert_eq!(val["status"], "generic");
        assert_eq!(val["title"], "Alloy");
        assert_eq!(val["body"], "New message");
        assert_eq!(val["room_id"], "!room:example.org");
        assert_eq!(val["event_id"], "$ev:example.org");
    }

    #[test]
    fn json_shapes_status_skip() {
        let outcome = NotificationOutcome::skip("filtered_out");
        let val = outcome.to_json_value();
        assert_eq!(val["status"], "skip");
        assert_eq!(val["reason"], "filtered_out");
        assert!(val.get("title").is_none());
    }

    #[test]
    fn json_shapes_status_error() {
        let outcome = NotificationOutcome::Error {
            error: "network failed".to_string(),
        };
        let val = outcome.to_json_value();
        assert_eq!(val["status"], "error");
        assert_eq!(val["error"], "network failed");
    }

    #[test]
    fn status_mapping_redacted_and_filtered() {
        let push_ref = PushRef {
            room_id: "!room:example.org".to_string(),
            event_id: "$ev:example.org".to_string(),
            unread: Some(1),
        };

        let outcome = map_notification_status(NotificationStatus::EventRedacted, &push_ref, None);
        assert_eq!(outcome, NotificationOutcome::skip("redacted"));

        let outcome =
            map_notification_status(NotificationStatus::EventFilteredOut, &push_ref, None);
        assert_eq!(outcome, NotificationOutcome::skip("filtered_out"));

        let outcome = map_notification_status(NotificationStatus::EventNotFound, &push_ref, None);
        assert!(matches!(outcome, NotificationOutcome::Generic { .. }));
    }

    #[test]
    fn pusher_config_deserialization() {
        let json = r#"{
            "app_id": "chat.alloy.android",
            "pushkey": "https://push.example.org/endpoint",
            "gateway_url": "https://gateway.example.org",
            "app_display_name": "Alloy",
            "device_display_name": "Pixel 8",
            "lang": "en"
        }"#;

        let config: PusherConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.app_id, "chat.alloy.android");
        assert_eq!(config.pushkey, "https://push.example.org/endpoint");
        assert_eq!(config.gateway_url, "https://gateway.example.org");
        assert_eq!(config.app_display_name, "Alloy");
        assert_eq!(config.device_display_name, "Pixel 8");
        assert_eq!(config.lang, "en");
    }
}
