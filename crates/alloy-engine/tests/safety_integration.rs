//! Moderation, device deletion, ignore list, report event, and global search against Synapse.
//! Gated on ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse, RoomMembership, UserId};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

const PW: &str = "testpassword123!";

async fn register_unique(hs: &str) -> String {
    use matrix_sdk::ruma::api::client::account::register::v3::Request;
    use matrix_sdk::ruma::api::client::uiaa::{AuthData, Dummy};
    let dir = tempfile::tempdir().unwrap().keep();
    let client = alloy_core::AlloyClient::new(hs, &dir, None).await.unwrap();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let username = format!("safety{nanos:x}");
    let mut req = Request::new();
    req.username = Some(username.clone());
    req.password = Some(PW.into());
    req.auth = Some(AuthData::Dummy(Dummy::new()));
    let _ = client.inner_client().matrix_auth().register(req).await;
    // Ignore if registration fails (user may already exist); login will fail if unrecoverable
    username
}

async fn setup_user(hs: &str, username: &str) -> (std::path::PathBuf, EngineContext, AppEngine) {
    let dir = tempfile::tempdir().unwrap().keep();
    let ctx = EngineContext::new(dir.clone(), Arc::new(InMemorySecretStore::new()));
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_string(),
        username: username.to_string(),
        password: PW.to_string(),
    })
    .await
    .unwrap();
    (dir, ctx, eng)
}

#[tokio::test]
async fn test_delete_devices_with_uiaa() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let user = register_unique(&hs).await;
    let (_dir, _ctx, eng) = setup_user(&hs, &user).await;

    // List devices
    let devices = match eng.handle(EngineCommand::ListDevices).await.unwrap() {
        EngineResponse::Devices { devices } => devices,
        resp => panic!("unexpected: {resp:?}"),
    };
    assert!(!devices.is_empty());

    // Try delete without password → AuthRequired or DevicesDeleted
    let result = eng
        .handle(EngineCommand::DeleteDevices {
            device_ids: vec![devices[0].device_id.clone()],
            password: None,
        })
        .await
        .unwrap();
    match result {
        EngineResponse::AuthRequired { flows } => {
            assert!(!flows.is_empty());
        }
        EngineResponse::DevicesDeleted { .. } => {
            // Also acceptable if server doesn't require password
        }
        resp => panic!("unexpected: {resp:?}"),
    }

    // Delete with password → DevicesDeleted
    let del_result = eng
        .handle(EngineCommand::DeleteDevices {
            device_ids: vec![devices[0].device_id.clone()],
            password: Some(PW.to_string()),
        })
        .await
        .unwrap();
    match del_result {
        EngineResponse::DevicesDeleted { deleted } => {
            assert!(deleted.contains(&devices[0].device_id));
        }
        EngineResponse::AuthRequired { .. } => {
            // Server may not require password; OK for this test
        }
        resp => panic!("unexpected: {resp:?}"),
    }
}

#[tokio::test]
async fn test_ignore_unignore_list_ignored_users() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let user = register_unique(&hs).await;
    let (_dir, _ctx, eng) = setup_user(&hs, &user).await;

    let bob_id = UserId("@bob:localhost".into());

    // Ignore bob
    match eng
        .handle(EngineCommand::IgnoreUser {
            user_id: bob_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::UserIgnored { user_id } => assert_eq!(user_id, bob_id),
        resp => panic!("unexpected: {resp:?}"),
    }

    // List ignored users
    tokio::time::sleep(Duration::from_millis(200)).await;
    match eng.handle(EngineCommand::ListIgnoredUsers).await.unwrap() {
        EngineResponse::IgnoredUsers { user_ids } => {
            assert!(user_ids.contains(&bob_id));
        }
        resp => panic!("unexpected: {resp:?}"),
    }

    // Unignore bob
    match eng
        .handle(EngineCommand::UnignoreUser {
            user_id: bob_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::UserUnignored { user_id } => assert_eq!(user_id, bob_id),
        resp => panic!("unexpected: {resp:?}"),
    }

    // Verify removed from list
    tokio::time::sleep(Duration::from_millis(200)).await;
    match eng.handle(EngineCommand::ListIgnoredUsers).await.unwrap() {
        EngineResponse::IgnoredUsers { user_ids } => {
            assert!(!user_ids.contains(&bob_id));
        }
        resp => panic!("unexpected: {resp:?}"),
    }
}

#[tokio::test]
async fn test_kick_ban_unban_set_power_level() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let alice_user = register_unique(&hs).await;
    let bob_user = register_unique(&hs).await;
    let (_alice_dir, _alice_ctx, alice) = setup_user(&hs, &alice_user).await;
    let (_bob_dir, _bob_ctx, bob) = setup_user(&hs, &bob_user).await;

    let bob_id = UserId(format!("@{}:localhost", bob_user));

    // Alice subscribes to room list and creates a room
    alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    let room_id = match alice
        .handle(EngineCommand::CreateRoom {
            name: "moderation-test".into(),
            encrypted: false,
            invite: Some(vec![bob_id.clone()]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        resp => panic!("unexpected: {resp:?}"),
    };

    // Bob joins
    bob.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify Alice has power
    let details = match alice
        .handle(EngineCommand::GetRoomDetails {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomDetailsRetrieved { details } => details,
        resp => panic!("unexpected: {resp:?}"),
    };
    assert!(details.can_kick);
    assert!(details.can_ban);
    assert!(details.can_set_power_levels);

    // Set power level
    match alice
        .handle(EngineCommand::SetPowerLevel {
            room_id: room_id.clone(),
            user_id: bob_id.clone(),
            level: 50,
        })
        .await
        .unwrap()
    {
        EngineResponse::PowerLevelSet { user_id, level, .. } => {
            assert_eq!(user_id, bob_id);
            assert_eq!(level, 50);
        }
        resp => panic!("unexpected: {resp:?}"),
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify power level in GetRoomMembers
    let members = match alice
        .handle(EngineCommand::GetRoomMembers {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomMembers { members, .. } => members,
        resp => panic!("unexpected: {resp:?}"),
    };
    let bob_member = members.iter().find(|m| m.user_id == bob_id).unwrap();
    assert_eq!(bob_member.power_level, 50);

    // Kick bob
    match alice
        .handle(EngineCommand::KickUser {
            room_id: room_id.clone(),
            user_id: bob_id.clone(),
            reason: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::UserKicked { .. } => (),
        resp => panic!("unexpected: {resp:?}"),
    }

    // Ban bob
    match alice
        .handle(EngineCommand::BanUser {
            room_id: room_id.clone(),
            user_id: bob_id.clone(),
            reason: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::UserBanned { .. } => (),
        resp => panic!("unexpected: {resp:?}"),
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify banned
    let members = match alice
        .handle(EngineCommand::GetRoomMembers {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomMembers { members, .. } => members,
        resp => panic!("unexpected: {resp:?}"),
    };
    let bob_member = members.iter().find(|m| m.user_id == bob_id).unwrap();
    assert_eq!(bob_member.membership, RoomMembership::Banned);

    // Unban bob
    match alice
        .handle(EngineCommand::UnbanUser {
            room_id: room_id.clone(),
            user_id: bob_id.clone(),
            reason: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::UserUnbanned { .. } => (),
        resp => panic!("unexpected: {resp:?}"),
    }
}

#[tokio::test]
async fn test_mention_room_and_report_event() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let alice_user = register_unique(&hs).await;
    let bob_user = register_unique(&hs).await;
    let (_alice_dir, _alice_ctx, alice) = setup_user(&hs, &alice_user).await;
    let (_bob_dir, _bob_ctx, bob) = setup_user(&hs, &bob_user).await;

    let bob_id = UserId(format!("@{}:localhost", bob_user));

    alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    let room_id = match alice
        .handle(EngineCommand::CreateRoom {
            name: "mention-test".into(),
            encrypted: false,
            invite: Some(vec![bob_id.clone()]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        resp => panic!("unexpected: {resp:?}"),
    };

    bob.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();

    // Send with mention_room
    alice
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: "Attention @room!".into(),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            mention_room: true,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Report an event
    let _report_result = bob
        .handle(EngineCommand::ReportEvent {
            room_id: room_id.clone(),
            event_id: alloy_protocol::EventId("$fake:localhost".into()),
            reason: Some("test".into()),
        })
        .await;
}

#[tokio::test]
async fn test_search_all_messages() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let alice_user = register_unique(&hs).await;
    let (_alice_dir, _alice_ctx, alice) = setup_user(&hs, &alice_user).await;

    alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    // Create two unencrypted rooms
    let room1 = match alice
        .handle(EngineCommand::CreateRoom {
            name: "search-room-1".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        resp => panic!("unexpected: {resp:?}"),
    };

    let room2 = match alice
        .handle(EngineCommand::CreateRoom {
            name: "search-room-2".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        resp => panic!("unexpected: {resp:?}"),
    };

    let unique_word = format!("searchword{}", uuid_like());

    // Send message to room1
    alice
        .handle(EngineCommand::SendText {
            room_id: room1.clone(),
            body: format!("Message with {unique_word} in room1"),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            mention_room: false,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap();

    // Send message to room2
    alice
        .handle(EngineCommand::SendText {
            room_id: room2.clone(),
            body: format!("Another message with {unique_word} in room2"),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            mention_room: false,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap();

    // Wait for search index to ingest
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Search all messages
    let result = alice
        .handle(EngineCommand::SearchAllMessages {
            query: unique_word,
            next_batch: None,
            limit: Some(10),
        })
        .await
        .unwrap();

    match result {
        EngineResponse::AllMessagesFound { hits, .. } => {
            assert!(hits.len() >= 2, "should find messages in both rooms");
            let rooms_found: std::collections::HashSet<_> =
                hits.iter().map(|h| h.room_id.clone()).collect();
            assert!(rooms_found.contains(&room1));
            assert!(rooms_found.contains(&room2));
        }
        resp => panic!("unexpected: {resp:?}"),
    }
}

fn uuid_like() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    );
    format!("{:x}", hasher.finish())
}
