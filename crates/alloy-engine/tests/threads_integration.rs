//! Thread timelines against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.
//! Accounts alice and bob use password `testpassword123!`.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, EventId, EventSendState, RoomId, SubscriptionId,
    TimelineItemContent, TimelineItemDto, UserId, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

const PASSWORD: &str = "testpassword123!";

/// Apply streamed diffs to a local mirror, as the UI does.
fn apply(list: &mut Vec<TimelineItemDto>, diff: VectorDiffDto<TimelineItemDto>) {
    match diff {
        VectorDiffDto::Append { values } => list.extend(values),
        VectorDiffDto::Clear => list.clear(),
        VectorDiffDto::PushFront { value } => list.insert(0, value),
        VectorDiffDto::PushBack { value } => list.push(value),
        VectorDiffDto::PopFront => {
            list.remove(0);
        }
        VectorDiffDto::PopBack => {
            list.pop();
        }
        VectorDiffDto::Insert { index, value } => list.insert(index, value),
        VectorDiffDto::Set { index, value } => list[index] = value,
        VectorDiffDto::Remove { index } => {
            list.remove(index);
        }
        VectorDiffDto::Truncate { length } => list.truncate(length),
        VectorDiffDto::Reset { values } => *list = values,
    }
}

/// Pump events into the mirror of whichever subscription they belong to until `done(mirrors)`
/// holds or the timeout hits. Every subscription's diffs must be applied, otherwise a mirror
/// that misses diffs drifts out of sync and later index-based diffs fail.
async fn until(
    rx: &mut Receiver<EngineEvent>,
    subs: &[&SubscriptionId],
    mirrors: &mut [Vec<TimelineItemDto>],
    what: &str,
    done: impl Fn(&[Vec<TimelineItemDto>]) -> bool,
) {
    let res = tokio::time::timeout(Duration::from_secs(60), async {
        while !done(mirrors) {
            if let EngineEvent::TimelineDiff {
                subscription_id,
                diff,
                ..
            } = rx.recv().await.unwrap()
                && let Some(i) = subs.iter().position(|s| **s == subscription_id)
            {
                apply(&mut mirrors[i], *diff);
            }
        }
    })
    .await;
    assert!(
        res.is_ok(),
        "timed out waiting for: {what}; mirrors={mirrors:#?}"
    );
}

fn text_of(i: &TimelineItemDto) -> Option<&str> {
    match &i.content {
        TimelineItemContent::TextMessage { body, .. } => Some(body),
        _ => None,
    }
}

fn find<'a>(m: &'a [TimelineItemDto], body: &str) -> Option<&'a TimelineItemDto> {
    m.iter().find(|i| text_of(i) == Some(body))
}

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

/// The room must be known to the client (after sync) before its timelines can be opened.
async fn subscribe_timeline(eng: &AppEngine, room_id: &RoomId) -> SubscriptionId {
    let mut attempts = 0;
    loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => return subscription_id,
            Err(e) if e.code == "not_found" && attempts < 30 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

async fn subscribe_thread(eng: &AppEngine, room_id: &RoomId, root: &EventId) -> SubscriptionId {
    let mut attempts = 0;
    loop {
        match eng
            .handle(EngineCommand::SubscribeThread {
                room_id: room_id.clone(),
                thread_root_event_id: root.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => return subscription_id,
            Err(e) if e.code == "not_found" && attempts < 30 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn thread_reply_reaches_thread_subscription_and_root_summary() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, alice) = login(&hs, "alice").await;
    let (_bob_dir, _bob_ctx, bob) = login(&hs, "bob").await;

    // Alice creates a room, invites bob, and bob joins.
    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("thread-test-{}", uuid::Uuid::new_v4().simple()),
            encrypted: false,
            invite: Some(vec![UserId("@bob:localhost".into())]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();

    // Both sides sync the room list before opening timelines.
    alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    bob.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    // Alice's room timeline: send the thread root and wait for the server echo.
    let mut alice_rx = alice_ctx.sink.subscribe();
    let main_sub = subscribe_timeline(&alice, &room_id).await;
    // mirrors[0] = room timeline, mirrors[1] = thread timeline (subscribed later).
    let mut mirrors: Vec<Vec<TimelineItemDto>> = vec![vec![], vec![]];
    alice
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: "thread root".into(),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap();
    until(
        &mut alice_rx,
        &[&main_sub],
        &mut mirrors,
        "root sent",
        |m| {
            find(&m[0], "thread root").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;
    let root_id: EventId = find(&mirrors[0], "thread root")
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    // Alice opens the thread; the root is part of its timeline.
    let alice_thread_sub = subscribe_thread(&alice, &room_id, &root_id).await;
    until(
        &mut alice_rx,
        &[&main_sub, &alice_thread_sub],
        &mut mirrors,
        "thread root",
        |m| find(&m[1], "thread root").is_some(),
    )
    .await;

    // Bob opens the same thread and replies in it.
    subscribe_thread(&bob, &room_id, &root_id).await;
    bob.handle(EngineCommand::SendText {
        room_id: room_id.clone(),
        body: "thread reply".into(),
        markdown: Some(false),
        reply_to: None,
        mentions: None,
        txn_id: None,
        in_thread_root: Some(root_id.clone()),
    })
    .await
    .unwrap();

    // The reply reaches alice's thread subscription, not as a loose room message.
    until(
        &mut alice_rx,
        &[&main_sub, &alice_thread_sub],
        &mut mirrors,
        "reply in thread",
        |m| find(&m[1], "thread reply").is_some_and(|i| !i.is_own),
    )
    .await;

    // The room timeline's root now reports one reply in its thread summary.
    until(
        &mut alice_rx,
        &[&main_sub, &alice_thread_sub],
        &mut mirrors,
        "root thread summary",
        |m| {
            find(&m[0], "thread root")
                .and_then(|i| i.thread_summary.as_ref())
                .is_some_and(|s| s.reply_count == 1)
        },
    )
    .await;
}
