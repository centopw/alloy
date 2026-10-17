//! Room list handler against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, RoomSummary, VectorDiffDto};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

fn contains_room(diff: &VectorDiffDto<RoomSummary>, name: &str) -> bool {
    let has = |r: &RoomSummary| r.name.as_deref() == Some(name);
    match diff {
        VectorDiffDto::Append { values } | VectorDiffDto::Reset { values } => {
            values.iter().any(has)
        }
        VectorDiffDto::PushFront { value }
        | VectorDiffDto::PushBack { value }
        | VectorDiffDto::Insert { value, .. }
        | VectorDiffDto::Set { value, .. } => has(value),
        _ => false,
    }
}

#[tokio::test]
async fn room_list_streams_new_room_and_unsubscribe_stops() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());

    // Not logged in -> typed auth error
    let err = eng
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap_err();
    assert_eq!(err.code, "auth_failed");

    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs,
        username: "alice".into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();

    let mut events = ctx.sink.subscribe();
    let sub = match eng
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    let name = format!("engine-test-{}", uuid_like());
    let created = eng
        .handle(EngineCommand::CreateRoom {
            name: name.clone(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap();
    let room_id = match created {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // The new room shows up in the streamed diffs, tagged with our subscription id.
    let found = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match events.recv().await.unwrap() {
                EngineEvent::RoomListDiff {
                    subscription_id,
                    diff,
                } if subscription_id == sub && contains_room(&diff, &name) => {
                    break;
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        found.is_ok(),
        "room {name} never appeared in the room list stream"
    );

    // Leaving the room works; unsubscribing cancels the task.
    eng.handle(EngineCommand::LeaveRoom { room_id })
        .await
        .unwrap();
    eng.handle(EngineCommand::Unsubscribe {
        subscription_id: sub.clone(),
    })
    .await
    .unwrap();
    assert!(!ctx.subscriptions.is_active(&sub).await);
}

fn uuid_like() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
