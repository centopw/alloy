//! Own profile, display name, device list, session info and media cache against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008. Without it every test skips.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;

fn engine(dir: &tempfile::TempDir) -> AppEngine {
    AppEngine::new(EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    ))
}

async fn login(eng: &AppEngine, hs: &str, username: &str) -> alloy_protocol::Account {
    match eng
        .handle(EngineCommand::LoginPassword {
            homeserver_url: hs.to_owned(),
            username: username.into(),
            password: "testpassword123!".into(),
        })
        .await
        .unwrap()
    {
        EngineResponse::LoggedIn { account } => account,
        other => panic!("unexpected login response: {other:?}"),
    }
}

async fn own_profile(eng: &AppEngine) -> alloy_protocol::OwnProfileDto {
    match eng.handle(EngineCommand::GetOwnProfile).await.unwrap() {
        EngineResponse::OwnProfile { profile } => profile,
        other => panic!("unexpected profile response: {other:?}"),
    }
}

#[tokio::test]
async fn own_profile_matches_login_account() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alice = engine(&dir);
    let account = login(&alice, &hs, "alice").await;

    let profile = own_profile(&alice).await;
    assert_eq!(profile.user_id.0, account.user_id.0);
    assert!(
        !profile.device_id.is_empty(),
        "profile must name this session's device"
    );
}

#[tokio::test]
async fn set_display_name_round_trips_and_rejects_blank_names() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alice = engine(&dir);
    login(&alice, &hs, "alice").await;

    let blank = alice
        .handle(EngineCommand::SetDisplayName { name: "   ".into() })
        .await;
    assert!(
        blank.is_err(),
        "a whitespace-only display name must be rejected"
    );

    // The protocol cannot clear a display name, so the account must already have one to restore.
    let original = own_profile(&alice)
        .await
        .display_name
        .expect("test account needs a display name so it can be restored");
    let renamed = format!("Alice Renamed {}", std::process::id());

    alice
        .handle(EngineCommand::SetDisplayName {
            name: renamed.clone(),
        })
        .await
        .unwrap();
    assert_eq!(own_profile(&alice).await.display_name, Some(renamed));

    alice
        .handle(EngineCommand::SetDisplayName {
            name: original.clone(),
        })
        .await
        .unwrap();
    assert_eq!(own_profile(&alice).await.display_name, Some(original));
}

#[tokio::test]
async fn list_devices_marks_only_this_session_as_current() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alice = engine(&dir);
    login(&alice, &hs, "alice").await;
    let profile = own_profile(&alice).await;

    let devices = match alice.handle(EngineCommand::ListDevices).await.unwrap() {
        EngineResponse::Devices { devices } => devices,
        other => panic!("unexpected devices response: {other:?}"),
    };
    let current: Vec<_> = devices.iter().filter(|d| d.is_current).collect();
    assert_eq!(
        current.len(),
        1,
        "exactly one device is the current session"
    );
    assert_eq!(current[0].device_id, profile.device_id);
    assert!(devices[0].is_current, "the current session is listed first");
}

#[tokio::test]
async fn session_info_reports_this_device_and_its_keys() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alice = engine(&dir);
    login(&alice, &hs, "alice").await;
    let profile = own_profile(&alice).await;

    let info = match alice.handle(EngineCommand::GetSessionInfo).await.unwrap() {
        EngineResponse::SessionInfo { info } => info,
        other => panic!("unexpected session response: {other:?}"),
    };
    assert_eq!(info.device_id, profile.device_id);
    assert!(info.ed25519_key.is_some_and(|key| !key.is_empty()));
    assert!(info.curve25519_key.is_some_and(|key| !key.is_empty()));
}

#[tokio::test]
async fn clear_media_cache_on_empty_cache_frees_nothing() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alice = engine(&dir);
    login(&alice, &hs, "alice").await;

    match alice.handle(EngineCommand::ClearMediaCache).await.unwrap() {
        EngineResponse::MediaCacheCleared { freed_bytes } => assert_eq!(freed_bytes, 0),
        other => panic!("unexpected media cache response: {other:?}"),
    }
}
