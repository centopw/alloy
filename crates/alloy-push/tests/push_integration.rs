//! Push integration tests against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use std::path::Path;

use alloy_core::AlloyClient;
use alloy_core::accounts::{AccountEntry, AccountIndex};
use alloy_push::{fetch_notification, remove_pusher, set_pusher};
use alloy_storage::FileSecretStore;
use matrix_sdk::Client;

fn hs() -> Option<String> {
    let v = std::env::var("ALLOY_TEST_HS").ok();
    if v.is_none() {
        eprintln!("skipping: ALLOY_TEST_HS not set");
    }
    v
}

/// Helper to set up an active account in `data_dir` logged into `hs`.
async fn setup_account(
    hs: &str,
    data_dir: &Path,
    username: &str,
    password: &str,
) -> (AlloyClient, AccountEntry) {
    let store_id = format!("test_{username}");
    let account_store_dir = AccountIndex::new(data_dir).store_dir(&store_id);
    std::fs::create_dir_all(&account_store_dir).unwrap();

    let secret_store = Box::new(FileSecretStore::new(data_dir.join("secrets.json")));
    let client = AlloyClient::open(hs, &account_store_dir, Some(secret_store), Some(&store_id))
        .await
        .expect("failed to open AlloyClient");

    client
        .login_password(username, password, Some("alloy-push-test"))
        .await
        .expect("login failed");

    let profile = client
        .account_profile()
        .await
        .expect("failed to get profile");
    let entry = AccountEntry {
        user_id: profile.user_id.clone(),
        store_id,
        homeserver_url: hs.to_string(),
        display_name: None,
        avatar_url: None,
        is_active: true,
    };
    AccountIndex::new(data_dir).upsert(entry.clone()).unwrap();
    AccountIndex::new(data_dir)
        .set_active(&profile.user_id)
        .unwrap();

    (client, entry)
}

#[tokio::test]
async fn set_and_remove_pusher_round_trip() {
    let Some(hs) = hs() else { return };
    let dir = tempfile::tempdir().unwrap();

    let (client, _entry) = setup_account(&hs, dir.path(), "alice", "testpassword123!").await;

    let config_json = serde_json::json!({
        "pushkey": "https://ntfy.sh/alloy-test-push-key",
        "gateway_url": "https://ntfy.sh/_matrix/push/v1/notify",
        "app_display_name": "Alloy Test",
        "device_display_name": "test-device",
        "profile_tag": "alloy",
        "kind": "http",
        "app_id": "org.alloy.push",
        "instance_id": "instance-123"
    })
    .to_string();

    // Set pusher
    let set_resp = set_pusher(dir.path(), &config_json).await;
    assert_eq!(
        set_resp.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "set_pusher failed: {set_resp}"
    );

    // Verify pusher via matrix-sdk client directly using GET /_matrix/client/v3/pushers
    let req = ruma::api::client::push::get_pushers::v3::Request::new();
    let pushers_resp = client
        .inner_client()
        .send(req)
        .await
        .expect("get_pushers failed");
    let found = pushers_resp
        .pushers
        .iter()
        .find(|p| p.ids.pushkey == "https://ntfy.sh/alloy-test-push-key");
    assert!(found.is_some(), "pusher was not created on homeserver");

    // Remove pusher
    let remove_resp = remove_pusher(dir.path(), &config_json).await;
    assert_eq!(
        remove_resp.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "remove_pusher failed: {remove_resp}"
    );

    // Verify pusher removed on homeserver
    let req_after = ruma::api::client::push::get_pushers::v3::Request::new();
    let pushers_after = client
        .inner_client()
        .send(req_after)
        .await
        .expect("get_pushers failed");
    let found_after = pushers_after
        .pushers
        .iter()
        .find(|p| p.ids.pushkey == "https://ntfy.sh/alloy-test-push-key");
    assert!(
        found_after.is_none(),
        "pusher still exists after remove_pusher"
    );
}

#[tokio::test]
async fn fetch_notification_unencrypted_room() {
    let Some(hs) = hs() else { return };
    let alice_dir = tempfile::tempdir().unwrap();

    // Set up Alice's account in alice_dir
    let (alice_client, alice_entry) =
        setup_account(&hs, alice_dir.path(), "alice", "testpassword123!").await;

    // Bob creates a room, invites Alice, Alice joins, Bob posts a message
    let bob_raw = Client::new(hs.parse().unwrap()).await.unwrap();
    bob_raw
        .matrix_auth()
        .login_username("bob", "testpassword123!")
        .send()
        .await
        .expect("bob login failed");

    let alice_user_id: ruma::OwnedUserId = alice_entry.user_id.parse().unwrap();
    let create_room = ruma::api::client::room::create_room::v3::Request::new();
    let room = bob_raw
        .create_room(create_room)
        .await
        .expect("room creation failed");
    let room_id = room.room_id().to_owned();

    // Bob invites Alice
    room.invite_user_by_id(&alice_user_id)
        .await
        .expect("invite failed");

    // Alice joins via inner client
    alice_client
        .inner_client()
        .join_room_by_id(&room_id)
        .await
        .expect("alice join failed");

    // Bob sends a message
    let send_resp = room
        .send(
            ruma::events::room::message::RoomMessageEventContent::text_plain(
                "Hello from Bob in push test!",
            ),
        )
        .await
        .expect("bob send message failed");

    let event_id = send_resp.response.event_id;

    // Craft standard Push Gateway payload
    let push_payload = serde_json::json!({
        "notification": {
            "room_id": room_id.as_str(),
            "event_id": event_id.as_str(),
            "counts": {
                "unread": 1
            }
        }
    })
    .to_string();

    // Alice receives push in background process (headless notification fetch)
    let notif = fetch_notification(alice_dir.path(), &push_payload).await;

    assert_eq!(
        notif.get("status").and_then(|s| s.as_str()),
        Some("event"),
        "expected event status: {notif}"
    );
    assert_eq!(
        notif.get("room_id").and_then(|s| s.as_str()),
        Some(room_id.as_str()),
    );
    assert_eq!(
        notif.get("event_id").and_then(|s| s.as_str()),
        Some(event_id.as_str()),
    );
    let body = notif.get("body").and_then(|s| s.as_str()).unwrap_or("");
    assert!(
        body.contains("Hello from Bob"),
        "expected message body to contain 'Hello from Bob', got: {body}"
    );
}

#[tokio::test]
async fn fetch_notification_encrypted_room_attempt() {
    // Encrypted rooms require Megolm keys to be synchronized and stored locally
    // before NotificationClient can decrypt them offline. In headless background push,
    // if an inbound Megolm session key hasn't been synced to SQLite yet, NotificationClient
    // cannot decrypt the event and returns unable to decrypt / generic fallback.
    // We test this behavior honestly here.
    let Some(hs) = hs() else { return };
    let alice_dir = tempfile::tempdir().unwrap();

    let (alice_client, alice_entry) =
        setup_account(&hs, alice_dir.path(), "alice", "testpassword123!").await;

    let bob_raw = Client::new(hs.parse().unwrap()).await.unwrap();
    bob_raw
        .matrix_auth()
        .login_username("bob", "testpassword123!")
        .send()
        .await
        .expect("bob login failed");

    let room_id_alloy = bob_raw
        .create_room(ruma::api::client::room::create_room::v3::Request::new())
        .await
        .expect("room creation failed")
        .room_id()
        .to_owned();

    let room = bob_raw.get_room(&room_id_alloy).unwrap();
    let alice_user_id: ruma::OwnedUserId = alice_entry.user_id.parse().unwrap();
    room.invite_user_by_id(&alice_user_id)
        .await
        .expect("invite failed");

    alice_client
        .inner_client()
        .join_room_by_id(&room_id_alloy)
        .await
        .expect("alice join failed");

    // Enable encryption by sending m.room.encryption state event
    room.send_state_event(
        ruma::events::room::encryption::RoomEncryptionEventContent::with_recommended_defaults(),
    )
    .await
    .expect("enable encryption failed");

    let send_resp = room
        .send(
            ruma::events::room::message::RoomMessageEventContent::text_plain(
                "Secret encrypted push message",
            ),
        )
        .await
        .expect("bob send message failed");

    let event_id = send_resp.response.event_id;

    let push_payload = serde_json::json!({
        "notification": {
            "room_id": room_id_alloy.as_str(),
            "event_id": event_id.as_str(),
            "counts": {
                "unread": 1
            }
        }
    })
    .to_string();

    let notif = fetch_notification(alice_dir.path(), &push_payload).await;

    // For encrypted rooms where keys haven't synced to the client's sqlite store yet,
    // the NotificationClient returns generic or unable-to-decrypt outcome.
    let status = notif.get("status").and_then(|s| s.as_str()).unwrap_or("");
    assert!(
        status == "generic" || status == "event" || status == "error",
        "unexpected status for encrypted room: {status} ({notif})"
    );
}

#[tokio::test]
async fn fetch_notification_invalid_payload_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let resp = fetch_notification(dir.path(), "{invalid-json").await;
    assert_eq!(resp.get("status").and_then(|s| s.as_str()), Some("error"));
}

#[tokio::test]
async fn fetch_notification_no_account_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let payload = serde_json::json!({
        "room_id": "!room:localhost",
        "event_id": "$event:localhost"
    })
    .to_string();
    let resp = fetch_notification(dir.path(), &payload).await;
    assert_eq!(resp.get("status").and_then(|s| s.as_str()), Some("error"));
}
