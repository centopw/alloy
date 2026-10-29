//! Unread counts against a local Synapse: a room with unread messages is cleared by a read
//! receipt for its newest event and by "mark room read". Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, RoomId, RoomSummary, SubscriptionId,
    TimelineItemContent, TimelineItemDto, UserId, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

const PASSWORD: &str = "testpassword123!";

/// Upper bound for every event-stream wait in this suite. The waits are event-driven; this only
/// catches a hang, so it is well above the observed runtime instead of just above it.
const WAIT_TIMEOUT: Duration = Duration::from_secs(180);

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

/// Mirror of the room list, kept the way the UI keeps it.
fn apply_room(list: &mut Vec<RoomSummary>, diff: VectorDiffDto<RoomSummary>) {
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

fn count_of(rooms: &[RoomSummary], room_id: &RoomId) -> Option<u64> {
    rooms
        .iter()
        .find(|r| &r.id == room_id)
        .map(|r| r.unread_notifications.notification_count)
}

/// Pump room-list and timeline events into their mirrors until `done` holds.
///
/// The bound is generous on purpose: the waits poll the event stream, so it only guards against a
/// hang. A tight budget makes the suite flake on a loaded CI box, where a full round already takes
/// over a minute on an idle machine.
async fn pump_until(
    rx: &mut Receiver<EngineEvent>,
    room_sub: &SubscriptionId,
    tl_sub: &SubscriptionId,
    rooms: &mut Vec<RoomSummary>,
    items: &mut Vec<TimelineItemDto>,
    what: &str,
    done: impl Fn(&[RoomSummary], &[TimelineItemDto]) -> bool,
) {
    let res = tokio::time::timeout(WAIT_TIMEOUT, async {
        while !done(rooms, items) {
            match rx.recv().await.unwrap() {
                EngineEvent::RoomListDiff {
                    subscription_id,
                    diff,
                } if &subscription_id == room_sub => apply_room(rooms, diff),
                EngineEvent::TimelineDiff {
                    subscription_id,
                    diff,
                    ..
                } if &subscription_id == tl_sub => apply_item(items, *diff),
                _ => {}
            }
        }
    })
    .await;
    assert!(res.is_ok(), "timed out waiting for: {what}");
}

fn apply_item(list: &mut Vec<TimelineItemDto>, diff: VectorDiffDto<TimelineItemDto>) {
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

fn has_text(items: &[TimelineItemDto], text: &str) -> bool {
    items.iter().any(
        |i| matches!(&i.content, TimelineItemContent::TextMessage { body, .. } if body == text),
    )
}

#[tokio::test]
async fn read_receipt_and_mark_room_read_clear_unread_count() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, alice) = login(&hs, "alice").await;
    let (_bob_dir, _bob_ctx, bob) = login(&hs, "bob").await;

    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("unread-test-{}", uuid::Uuid::new_v4().simple()),
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

    let mut rx = alice_ctx.sink.subscribe();
    let room_sub = match alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };
    bob.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    let _bob_timeline = subscribe_timeline(&bob, &room_id).await;
    let tl_sub = subscribe_timeline(&alice, &room_id).await;
    let mut rooms: Vec<RoomSummary> = vec![];
    let mut items: Vec<TimelineItemDto> = vec![];

    for round in 0..2 {
        for n in 0..3 {
            bob.handle(EngineCommand::SendText {
                room_id: room_id.clone(),
                body: format!("unread {round}-{n}"),
                markdown: Some(false),
                reply_to: None,
                mentions: None,
                mention_room: false,
                in_thread_root: None,
                txn_id: None,
            })
            .await
            .unwrap();
        }
        let newest = format!("unread {round}-2");
        pump_until(
            &mut rx,
            &room_sub,
            &tl_sub,
            &mut rooms,
            &mut items,
            "the new messages and an unread count",
            |rooms, items| has_text(items, &newest) && count_of(rooms, &room_id) > Some(0),
        )
        .await;

        if round == 0 {
            // The UI path: the newest event is on screen, so report it as read.
            let last = items
                .iter()
                .rev()
                .find_map(|i| i.event_id.clone())
                .expect("a confirmed event");
            alice
                .handle(EngineCommand::MarkRead {
                    room_id: room_id.clone(),
                    event_id: last,
                })
                .await
                .unwrap();
        } else {
            // The context-menu path.
            alice
                .handle(EngineCommand::MarkRoomRead {
                    room_id: room_id.clone(),
                })
                .await
                .unwrap();
        }
        pump_until(
            &mut rx,
            &room_sub,
            &tl_sub,
            &mut rooms,
            &mut items,
            if round == 0 {
                "a read receipt to clear the unread count"
            } else {
                "mark room read to clear the unread count"
            },
            |rooms, _| count_of(rooms, &room_id) == Some(0),
        )
        .await;
    }
}

/// Waits until another member of the room sees `reader`'s public read receipt at `expected`.
///
/// Receipts reach other clients through sync, so this proves the server accepted the receipt for
/// that exact event — not merely that the engine call returned `Ok`.
async fn wait_for_receipt(
    client: &alloy_core::AlloyClient,
    room_id: &RoomId,
    reader: &str,
    expected: &str,
) {
    use matrix_sdk::ruma::events::receipt::{ReceiptThread, ReceiptType};
    let rid = matrix_sdk::ruma::RoomId::parse(&room_id.0).unwrap();
    let uid = matrix_sdk::ruma::UserId::parse(reader).unwrap();
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let found = match client.inner_client().get_room(&rid) {
            Some(room) => room
                .load_user_receipt(ReceiptType::Read, &ReceiptThread::Unthreaded, &uid)
                .await
                .unwrap(),
            None => None,
        };
        if let Some((event_id, _)) = found
            && event_id.as_str() == expected
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{reader}'s receipt at {expected} never reached the other member"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// T9a: `MarkRead` must put a public `m.read` receipt on the wire for the exact event it was
/// given, including the account's own newest event. The SDK's timeline helper either redirects a
/// receipt off the user's own events or skips it entirely and still reports success, so the
/// engine sends the receipt itself and returns a failure when the request fails.
#[tokio::test]
async fn mark_read_publishes_a_receipt_for_peer_and_own_events() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, alice) = login(&hs, "alice").await;
    let (_bob_dir, bob_ctx, bob) = login(&hs, "bob").await;

    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("receipt-test-{}", uuid::Uuid::new_v4().simple()),
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

    let alice_id = alice_ctx
        .active_client()
        .await
        .unwrap()
        .user_id()
        .unwrap()
        .to_owned();
    let bob_client = bob_ctx.active_client().await.unwrap();

    let mut rx = alice_ctx.sink.subscribe();
    let room_sub = match alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };
    let tl_sub = subscribe_timeline(&alice, &room_id).await;
    let mut rooms: Vec<RoomSummary> = vec![];
    let mut items: Vec<TimelineItemDto> = vec![];

    // A peer's message: the receipt goes to that event.
    bob.handle(EngineCommand::SendText {
        room_id: room_id.clone(),
        body: "peer receipt".into(),
        markdown: Some(false),
        reply_to: None,
        mentions: None,
        mention_room: false,
        in_thread_root: None,
        txn_id: None,
    })
    .await
    .unwrap();
    pump_until(
        &mut rx,
        &room_sub,
        &tl_sub,
        &mut rooms,
        &mut items,
        "bob's message",
        |_, items| has_text(items, "peer receipt"),
    )
    .await;
    let peer_event = items
        .iter()
        .rev()
        .find_map(|i| i.event_id.clone())
        .expect("bob's confirmed event");
    alice
        .handle(EngineCommand::MarkRead {
            room_id: room_id.clone(),
            event_id: peer_event.clone(),
        })
        .await
        .unwrap();
    wait_for_receipt(&bob_client, &room_id, &alice_id, &peer_event.0).await;

    // The account's own newest event: the receipt must point at it, not be redirected and
    // silently dropped. This is the "sent a message, so it is read" case from the report.
    alice
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: "own receipt".into(),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            mention_room: false,
            in_thread_root: None,
            txn_id: None,
        })
        .await
        .unwrap();
    pump_until(
        &mut rx,
        &room_sub,
        &tl_sub,
        &mut rooms,
        &mut items,
        "alice's own message",
        |_, items| has_text(items, "own receipt"),
    )
    .await;
    let own_event = items
        .iter()
        .rev()
        .find_map(|i| i.event_id.clone())
        .expect("alice's confirmed event");
    alice
        .handle(EngineCommand::MarkRead {
            room_id: room_id.clone(),
            event_id: own_event.clone(),
        })
        .await
        .unwrap();
    wait_for_receipt(&bob_client, &room_id, &alice_id, &own_event.0).await;
}
