//! Room members and details against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse, RoomId, RoomMembership, UserId};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;

#[tokio::test]
async fn members_and_details_of_a_new_room() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx);

    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs,
        username: "alice".into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();

    let room_id = match eng
        .handle(EngineCommand::CreateRoom {
            name: "members-test".into(),
            encrypted: true,
            invite: Some(vec![UserId("@bob:localhost".into())]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Power levels arrive with the room state over sync; wait until the creator shows as admin.
    let members = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let members = match eng
                .handle(EngineCommand::GetRoomMembers {
                    room_id: room_id.clone(),
                })
                .await
                .unwrap()
            {
                EngineResponse::RoomMembers {
                    room_id: rid,
                    members,
                } => {
                    assert_eq!(rid, room_id);
                    members
                }
                other => panic!("unexpected {other:?}"),
            };
            if members.iter().any(|m| m.power_level >= 100) {
                break members;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("room creator never showed an admin power level");
    assert_eq!(members.len(), 2, "{members:?}");
    // Highest power level first: alice created the room.
    assert_eq!(members[0].user_id.0, "@alice:localhost");
    assert_eq!(members[0].membership, RoomMembership::Joined);
    assert!(members[0].power_level >= 100);
    assert_eq!(members[1].user_id.0, "@bob:localhost");
    assert_eq!(members[1].membership, RoomMembership::Invited);
    assert_eq!(members[1].power_level, 0);

    let details = match eng
        .handle(EngineCommand::GetRoomDetails {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomDetailsRetrieved { details } => details,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(details.room_id, room_id);
    assert_eq!(details.name.as_deref(), Some("members-test"));
    assert!(details.encrypted);
    assert!(!details.is_direct);
    assert_eq!(details.joined_members, 1);
    assert_eq!(details.invited_members, 1);

    let unknown = RoomId("!nope:localhost".into());
    let err = eng
        .handle(EngineCommand::GetRoomMembers { room_id: unknown })
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
}
