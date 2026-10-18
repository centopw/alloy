//! Public room directory search against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{AccountId, EngineCommand, EngineResponse, PublicRoomDto};
use alloy_storage::InMemorySecretStore;
use matrix_sdk::ruma::api::client::room::Visibility;
use matrix_sdk::ruma::api::client::room::create_room::v3::{
    Request as CreateRoomRequest, RoomPreset,
};
use std::sync::Arc;
use std::time::Duration;

const PASSWORD: &str = "testpassword123!";

/// Logs in `user` on a fresh engine and returns its context, engine and account id.
async fn login(hs: &str, user: &str) -> (tempfile::TempDir, EngineContext, AppEngine, AccountId) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    let account = match eng
        .handle(EngineCommand::LoginPassword {
            homeserver_url: hs.to_string(),
            username: user.to_string(),
            password: PASSWORD.to_string(),
        })
        .await
        .unwrap()
    {
        EngineResponse::LoggedIn { account } => account,
        other => panic!("unexpected {other:?}"),
    };
    (dir, ctx, eng, AccountId(account.user_id.0))
}

async fn search(
    eng: &AppEngine,
    account_id: &AccountId,
    query: &str,
) -> (Vec<PublicRoomDto>, Option<String>) {
    match eng
        .handle(EngineCommand::SearchPublicRooms {
            account_id: account_id.clone(),
            query: Some(query.to_string()),
            server: None,
            since: None,
            limit: 20,
        })
        .await
        .unwrap()
    {
        EngineResponse::PublicRooms {
            rooms, next_batch, ..
        } => (rooms, next_batch),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn public_directory_search_finds_published_room() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, _alice, _alice_id) = login(&hs, "alice").await;
    let (_bob_dir, _bob_ctx, bob, bob_id) = login(&hs, "bob").await;

    let name = format!("dir-test-{}", uuid::Uuid::new_v4().simple());

    // Alice publishes a public room.
    let alice_client = alice_ctx.active_client().await.unwrap();
    let mut request = CreateRoomRequest::new();
    request.name = Some(name.clone());
    request.visibility = Visibility::Public;
    request.preset = Some(RoomPreset::PublicChat);
    let created = alice_client
        .inner_client()
        .create_room(request)
        .await
        .unwrap();
    let room_id = created.room_id().to_string();

    // Directory propagation is asynchronous on the homeserver; poll briefly.
    let mut found = None;
    for _ in 0..20 {
        let (rooms, _) = search(&bob, &bob_id, &name).await;
        if let Some(room) = rooms.into_iter().find(|r| r.room_id.0 == room_id) {
            found = Some(room);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let room = found.expect("published room should appear in the public directory");
    assert_eq!(room.name.as_deref(), Some(name.as_str()));
    assert!(!room.joined, "bob has not joined the room yet");

    // Bob joins; the directory should then report the room as joined.
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.clone(),
    })
    .await
    .unwrap();
    let mut joined = false;
    for _ in 0..20 {
        let (rooms, _) = search(&bob, &bob_id, &name).await;
        if rooms.iter().any(|r| r.room_id.0 == room_id && r.joined) {
            joined = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(joined, "room should be reported as joined after bob joins");

    // A query matching nothing returns no rooms and no further pages.
    let nonsense = format!("zz-no-match-{}", uuid::Uuid::new_v4().simple());
    let (rooms, next_batch) = search(&bob, &bob_id, &nonsense).await;
    assert!(rooms.is_empty());
    assert!(next_batch.is_none());

    // Searching on behalf of an account that is not active is rejected.
    let err = bob
        .handle(EngineCommand::SearchPublicRooms {
            account_id: AccountId("@not-active:localhost".to_string()),
            query: Some(name),
            server: None,
            since: None,
            limit: 20,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
}
