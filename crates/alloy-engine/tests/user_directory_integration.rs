//! User-directory search against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse, UserSearchResultDto};
use alloy_storage::InMemorySecretStore;
use matrix_sdk::ruma::api::client::room::Visibility;
use matrix_sdk::ruma::api::client::room::create_room::v3::{
    Request as CreateRoomRequest, RoomPreset,
};
use std::sync::Arc;
use std::time::Duration;

const PASSWORD: &str = "testpassword123!";

/// Logs in `user` on a fresh engine and returns its context and engine.
async fn login(hs: &str, user: &str) -> (tempfile::TempDir, EngineContext, AppEngine) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    match eng
        .handle(EngineCommand::LoginPassword {
            homeserver_url: hs.to_string(),
            username: user.to_string(),
            password: PASSWORD.to_string(),
        })
        .await
        .unwrap()
    {
        EngineResponse::LoggedIn { .. } => (dir, ctx, eng),
        other => panic!("unexpected {other:?}"),
    }
}

async fn search(eng: &AppEngine, query: &str, limit: u32) -> (Vec<UserSearchResultDto>, bool) {
    match eng
        .handle(EngineCommand::SearchUsers {
            query: query.to_string(),
            limit,
        })
        .await
        .unwrap()
    {
        EngineResponse::UserSearchResults { results, limited } => (results, limited),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn user_directory_search_finds_a_shared_room_member() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, alice) = login(&hs, "alice").await;
    let (_bob_dir, bob_ctx, _bob) = login(&hs, "bob").await;

    // Bob publishes a room and Alice joins it, so the two accounts share a room.
    let name = format!("user-dir-test-{}", uuid::Uuid::new_v4().simple());
    let bob_client = bob_ctx.active_client().await.unwrap();
    let mut request = CreateRoomRequest::new();
    request.name = Some(name);
    request.visibility = Visibility::Public;
    request.preset = Some(RoomPreset::PublicChat);
    let room_id = bob_client
        .inner_client()
        .create_room(request)
        .await
        .unwrap()
        .room_id()
        .to_owned();

    let alice_client = alice_ctx.active_client().await.unwrap();
    alice_client
        .inner_client()
        .join_room_by_id(&room_id)
        .await
        .unwrap();

    // Alice's local membership state fills in through sync; poll briefly.
    let mut bob_hit = None;
    for _ in 0..20 {
        let (results, _) = search(&alice, "bob", 10).await;
        if let Some(result) = results
            .iter()
            .find(|r| r.user_id.0.starts_with("@bob:"))
            .filter(|r| r.already_in_room)
        {
            bob_hit = Some(result.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let bob_hit = bob_hit.expect("bob should be found and flagged as sharing a room");

    assert!(bob_hit.user_id.0.starts_with("@bob:"));
    assert!(
        bob_hit.already_in_room,
        "bob joins the same room as alice, so already_in_room must be true"
    );
}

#[tokio::test]
async fn user_directory_search_rejects_a_blank_query() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, _alice_ctx, alice) = login(&hs, "alice").await;

    let err = alice
        .handle(EngineCommand::SearchUsers {
            query: "   ".to_string(),
            limit: 10,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument");
    assert!(err.message.contains("blank"), "unexpected message: {err:?}");
}

#[tokio::test]
async fn user_directory_search_returns_no_matches_for_an_unknown_name() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, _alice_ctx, alice) = login(&hs, "alice").await;

    let (results, limited) = search(
        &alice,
        &format!("zz-no-such-user-{}", uuid::Uuid::new_v4().simple()),
        10,
    )
    .await;
    assert!(results.is_empty());
    assert!(!limited);
}
