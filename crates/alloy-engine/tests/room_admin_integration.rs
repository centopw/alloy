//! Room admin commands, profiles and direct messages against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse, RoomId, UserId};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

fn engine(dir: &tempfile::TempDir) -> AppEngine {
    AppEngine::new(EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    ))
}

async fn login(eng: &AppEngine, hs: &str, username: &str) {
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_owned(),
        username: username.into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();
    // Room state reaches the store through sync, which the room list starts.
    eng.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
}

/// Polls `GetRoomDetails` until `ready` accepts the response, failing after `WAIT`.
/// Errors while the room is still syncing count as "not ready yet".
async fn await_details(
    eng: &AppEngine,
    room_id: &RoomId,
    ready: impl Fn(&EngineResponse) -> bool,
) -> EngineResponse {
    tokio::time::timeout(WAIT, async {
        loop {
            let result = eng
                .handle(EngineCommand::GetRoomDetails {
                    room_id: room_id.clone(),
                })
                .await;
            if let Ok(response) = result
                && ready(&response)
            {
                break response;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("room state never reached the expected value")
}

#[tokio::test]
async fn room_admin_commands_against_synapse() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let alice_dir = tempfile::tempdir().unwrap();
    let bob_dir = tempfile::tempdir().unwrap();
    let alice = engine(&alice_dir);
    let bob = engine(&bob_dir);
    login(&alice, &hs, "alice").await;
    login(&bob, &hs, "bob").await;

    let room_id = match alice
        .handle(EngineCommand::CreateRoom {
            name: "admin-test".into(),
            encrypted: false,
            invite: Some(vec![UserId("@bob:localhost".into())]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Alice's power levels arrive over sync; wait until she may change name, topic and avatar.
    await_details(&alice, &room_id, |r| {
        matches!(r, EngineResponse::RoomDetailsRetrieved { details }
            if details.can_set_name && details.can_set_topic && details.can_set_avatar)
    })
    .await;

    // Bob joins the invite and waits until his sync shows the room with no admin rights.
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();
    await_details(&bob, &room_id, |r| {
        matches!(r, EngineResponse::RoomDetailsRetrieved { details }
            if !details.can_set_name && !details.can_set_topic && !details.can_set_avatar)
    })
    .await;

    let err = bob
        .handle(EngineCommand::SetRoomName {
            room_id: room_id.clone(),
            name: "hijack".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "forbidden", "{err:?}");

    assert!(matches!(
        alice
            .handle(EngineCommand::SetRoomName {
                room_id: room_id.clone(),
                name: "renamed".into(),
            })
            .await
            .unwrap(),
        EngineResponse::Unit
    ));
    assert!(matches!(
        alice
            .handle(EngineCommand::SetRoomTopic {
                room_id: room_id.clone(),
                topic: "new topic".into(),
            })
            .await
            .unwrap(),
        EngineResponse::Unit
    ));
    await_details(&alice, &room_id, |r| {
        matches!(r, EngineResponse::RoomDetailsRetrieved { details }
            if details.name.as_deref() == Some("renamed")
                && details.topic.as_deref() == Some("new topic"))
    })
    .await;

    // Private room note: alice's note round-trips, bob's stays empty.
    alice
        .handle(EngineCommand::SetRoomNote {
            room_id: room_id.clone(),
            note: "remember the agenda".into(),
        })
        .await
        .unwrap();
    match alice
        .handle(EngineCommand::GetRoomNote {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomNote { room_id: rid, note } => {
            assert_eq!(rid, room_id);
            assert_eq!(note, "remember the agenda");
        }
        other => panic!("unexpected {other:?}"),
    }
    match bob
        .handle(EngineCommand::GetRoomNote {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomNote { note, .. } => assert_eq!(note, ""),
        other => panic!("unexpected {other:?}"),
    }
    let too_long = "x".repeat(4001);
    let err = alice
        .handle(EngineCommand::SetRoomNote {
            room_id: room_id.clone(),
            note: too_long,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument", "{err:?}");

    // Profiles: bob seen from alice's room, and alice seeing herself.
    match alice
        .handle(EngineCommand::GetUserProfile {
            user_id: UserId("@bob:localhost".into()),
            room_id: Some(room_id.clone()),
        })
        .await
        .unwrap()
    {
        EngineResponse::UserProfile { profile } => {
            assert_eq!(profile.user_id.0, "@bob:localhost");
            assert!(!profile.is_me);
        }
        other => panic!("unexpected {other:?}"),
    }
    match alice
        .handle(EngineCommand::GetUserProfile {
            user_id: UserId("@alice:localhost".into()),
            room_id: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::UserProfile { profile } => assert!(profile.is_me),
        other => panic!("unexpected {other:?}"),
    }

    // Direct messages: accounts persist across runs, so the first call may reuse a room from an
    // earlier run; its `created` flag is therefore not asserted. Later calls must reuse that room.
    let dm_room = match alice
        .handle(EngineCommand::StartDirectMessage {
            user_id: UserId("@bob:localhost".into()),
        })
        .await
        .unwrap()
    {
        EngineResponse::DirectMessageRoom { room_id, .. } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    match alice
        .handle(EngineCommand::StartDirectMessage {
            user_id: UserId("@bob:localhost".into()),
        })
        .await
        .unwrap()
    {
        EngineResponse::DirectMessageRoom { room_id, created } => {
            assert_eq!(room_id, dm_room);
            assert!(!created);
        }
        other => panic!("unexpected {other:?}"),
    }
    // Two concurrent calls cover the double-click race that the per-user DM lock serialises.
    let start_dm = || {
        alice.handle(EngineCommand::StartDirectMessage {
            user_id: UserId("@bob:localhost".into()),
        })
    };
    let (first, second) = tokio::join!(start_dm(), start_dm());
    for result in [first, second] {
        match result.unwrap() {
            EngineResponse::DirectMessageRoom { room_id, created } => {
                assert_eq!(room_id, dm_room);
                assert!(!created);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let err = alice
        .handle(EngineCommand::StartDirectMessage {
            user_id: UserId("@alice:localhost".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument", "{err:?}");

    assert!(matches!(
        alice
            .handle(EngineCommand::MarkRoomRead {
                room_id: room_id.clone(),
            })
            .await
            .unwrap(),
        EngineResponse::Unit
    ));
}
