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
async fn pump_until(
    rx: &mut Receiver<EngineEvent>,
    room_sub: &SubscriptionId,
    tl_sub: &SubscriptionId,
    rooms: &mut Vec<RoomSummary>,
    items: &mut Vec<TimelineItemDto>,
    what: &str,
    done: impl Fn(&[RoomSummary], &[TimelineItemDto]) -> bool,
) {
    let res = tokio::time::timeout(Duration::from_secs(60), async {
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
