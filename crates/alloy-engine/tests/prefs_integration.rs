//! Synced preferences against a local Synapse: the account document round trip, rejection of
//! invalid documents, delivery to another signed-in device, and no echo to the writer. Set
//! ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast::Receiver;

const PASSWORD: &str = "testpassword123!";

async fn login(hs: &str, user: &str) -> (tempfile::TempDir, EngineContext, AppEngine) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_string(),
        username: user.to_string(),
        password: PASSWORD.to_string(),
    })
    .await
    .unwrap();
    (dir, ctx, eng)
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

/// A preferences document with the given `updated_at` and one visible setting.
fn prefs_doc(updated_at: u64, time_format: &str) -> String {
    serde_json::json!({ "updated_at": updated_at, "timeFormat": time_format }).to_string()
}

fn parsed(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap()
}

async fn get_prefs(eng: &AppEngine) -> Option<String> {
    match eng.handle(EngineCommand::GetAccountPrefs).await.unwrap() {
        EngineResponse::AccountPrefs { json } => json,
        other => panic!("unexpected {other:?}"),
    }
}

async fn set_prefs(eng: &AppEngine, json: String) {
    match eng
        .handle(EngineCommand::SetAccountPrefs { json })
        .await
        .unwrap()
    {
        EngineResponse::Unit => {}
        other => panic!("unexpected {other:?}"),
    }
}

async fn rejected(eng: &AppEngine, json: String) {
    let err = eng
        .handle(EngineCommand::SetAccountPrefs { json })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument", "message: {}", err.message);
}

/// The next preferences change on `rx` carrying `updated_at`, or `None` within `within`.
async fn prefs_change(
    rx: &mut Receiver<EngineEvent>,
    updated_at: u64,
    within: Duration,
) -> Option<String> {
    tokio::time::timeout(within, async {
        loop {
            match rx.recv().await.unwrap() {
                EngineEvent::AccountPrefsChanged { json }
                    if parsed(&json)["updated_at"].as_u64() == Some(updated_at) =>
                {
                    return json;
                }
                _ => {}
            }
        }
    })
    .await
    .ok()
}

#[tokio::test]
async fn account_prefs_round_trip_validate_and_sync_to_other_devices() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_da, a_ctx, alice_a) = login(&hs, "alice").await;
    let (_db, b_ctx, alice_b) = login(&hs, "alice").await;
    let mut a_rx = a_ctx.sink.subscribe();
    let mut b_rx = b_ctx.sink.subscribe();

    // Invalid documents are rejected and leave the stored one untouched.
    let before = get_prefs(&alice_a).await;
    rejected(&alice_a, "not json".into()).await;
    rejected(&alice_a, "[1,2]".into()).await;
    rejected(&alice_a, r#"{"timeFormat":"24h"}"#.into()).await;
    rejected(&alice_a, r#"{"updated_at":"1","timeFormat":"24h"}"#.into()).await;
    rejected(
        &alice_a,
        serde_json::json!({ "updated_at": now_ms(), "pad": "x".repeat(64 * 1024) }).to_string(),
    )
    .await;
    assert_eq!(get_prefs(&alice_a).await, before);

    // A write on device A reaches device B, and device A is not notified of its own write.
    let first = now_ms();
    set_prefs(&alice_a, prefs_doc(first, "24h")).await;
    let got = prefs_change(&mut b_rx, first, Duration::from_secs(60))
        .await
        .expect("device B never received the change");
    assert_eq!(parsed(&got)["timeFormat"], "24h");
    assert!(
        prefs_change(&mut a_rx, first, Duration::from_secs(5))
            .await
            .is_none(),
        "device A was notified of its own write"
    );
    assert_eq!(
        parsed(&get_prefs(&alice_b).await.unwrap())["timeFormat"],
        "24h"
    );

    // A newer write from device B reaches device A.
    let second = first + 1;
    set_prefs(&alice_b, prefs_doc(second, "12h")).await;
    let got = prefs_change(&mut a_rx, second, Duration::from_secs(60))
        .await
        .expect("device A never received the change");
    assert_eq!(parsed(&got)["timeFormat"], "12h");
}
