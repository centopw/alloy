//! In-room message search against a local Synapse.
//! Accounts alice and bob use password `testpassword123!`.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineError, EngineResponse, RoomId, RoomSearchHit, RoomSearchScope, UserId,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

const PASSWORD: &str = "testpassword123!";

async fn login(hs: &str, user: &str) -> (tempfile::TempDir, AppEngine) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx);
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_string(),
        username: user.to_string(),
        password: PASSWORD.to_string(),
    })
    .await
    .unwrap();
    (dir, eng)
}

/// Waits until the client knows the room (it becomes visible after a sync round).
async fn wait_for_room(eng: &AppEngine, room_id: &RoomId) {
    for _ in 0..60 {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(_) => return,
            Err(e) if e.code == "not_found" => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    panic!("room {} never became known", room_id.0);
}

async fn send_text(eng: &AppEngine, room_id: &RoomId, body: &str) {
    eng.handle(EngineCommand::SendText {
        room_id: room_id.clone(),
        body: body.into(),
        markdown: None,
        reply_to: None,
        mentions: None,
        txn_id: None,
        in_thread_root: None,
    })
    .await
    .unwrap();
}

async fn search(
    eng: &AppEngine,
    room_id: &RoomId,
    query: &str,
    sender: Option<&UserId>,
    limit: u32,
    next_batch: Option<String>,
) -> Result<(Vec<RoomSearchHit>, Option<String>, RoomSearchScope), EngineError> {
    match eng
        .handle(EngineCommand::SearchRoomMessages {
            room_id: room_id.clone(),
            query: query.into(),
            sender: sender.cloned(),
            limit,
            next_batch,
        })
        .await?
    {
        EngineResponse::RoomSearchResults {
            hits,
            next_batch,
            scope,
        } => Ok((hits, next_batch, scope)),
        other => panic!("unexpected {other:?}"),
    }
}

/// Polls until the search index returns `expected` hits for `query` (indexing is asynchronous).
async fn wait_for_hits(
    eng: &AppEngine,
    room_id: &RoomId,
    query: &str,
    expected: usize,
) -> Vec<RoomSearchHit> {
    for _ in 0..60 {
        let (hits, _, scope) = search(eng, room_id, query, None, 50, None).await.unwrap();
        assert_eq!(scope, RoomSearchScope::Server);
        if hits.len() >= expected {
            return hits;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("search for {query:?} never returned {expected} hits");
}

#[tokio::test]
async fn room_search_finds_text_messages_by_sender_and_pages() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice) = login(&hs, "alice").await;
    let (_bob_dir, bob) = login(&hs, "bob").await;

    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("room-search-{}", uuid::Uuid::new_v4().simple()),
            encrypted: false,
            invite: Some(vec![UserId("@bob:localhost".to_string())]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    wait_for_room(&alice, &room_id).await;

    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();
    wait_for_room(&bob, &room_id).await;

    send_text(&alice, &room_id, "needle alpha").await;
    send_text(&bob, &room_id, "needle beta").await;
    send_text(&alice, &room_id, "haystack").await;

    let alice_id = UserId("@alice:localhost".to_string());
    let bob_id = UserId("@bob:localhost".to_string());

    // Both "needle" messages become searchable once Synapse indexes them.
    let hits = wait_for_hits(&alice, &room_id, "needle", 2).await;
    let mut bodies: Vec<&str> = hits.iter().map(|h| h.body.as_str()).collect();
    bodies.sort_unstable();
    assert_eq!(bodies, vec!["needle alpha", "needle beta"]);
    assert!(hits.iter().all(|h| h.timestamp_millis > 0));

    // Sender filter keeps only bob's message.
    let (hits, _, scope) = search(&alice, &room_id, "needle", Some(&bob_id), 50, None)
        .await
        .unwrap();
    assert_eq!(scope, RoomSearchScope::Server);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].body, "needle beta");
    assert_eq!(hits[0].sender, bob_id);

    // The last message is indexed asynchronously too; wait for it before asserting.
    let haystack = wait_for_hits(&alice, &room_id, "haystack", 1).await;
    assert_eq!(haystack.len(), 1);
    // A term that appears once returns one hit from alice.
    let (hits, _, _) = search(&alice, &room_id, "haystack", None, 50, None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].body, "haystack");
    assert_eq!(hits[0].sender, alice_id);

    // limit 1 returns one hit and a token; the next page returns the other message.
    let (first, token, _) = search(&alice, &room_id, "needle", None, 1, None)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    let token = token.expect("limit 1 should leave a next_batch token");
    let (second, _, _) = search(&alice, &room_id, "needle", None, 1, Some(token))
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(first[0].event_id, second[0].event_id);
    let mut paged = vec![first[0].body.clone(), second[0].body.clone()];
    paged.sort_unstable();
    assert_eq!(paged, vec!["needle alpha", "needle beta"]);

    // A blank query is rejected before reaching the server.
    let err = search(&alice, &room_id, "   ", None, 50, None)
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument");
}

#[tokio::test]
async fn room_search_is_unavailable_for_encrypted_rooms() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice) = login(&hs, "alice").await;

    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("room-search-enc-{}", uuid::Uuid::new_v4().simple()),
            encrypted: true,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    wait_for_room(&alice, &room_id).await;

    let (hits, next_batch, scope) = search(&alice, &room_id, "needle", None, 50, None)
        .await
        .unwrap();
    assert!(hits.is_empty());
    assert!(next_batch.is_none());
    assert_eq!(scope, RoomSearchScope::Unavailable);
}
