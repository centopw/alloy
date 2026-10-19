#![allow(clippy::collapsible_if, clippy::collapsible_match)]

//! Chat parity integration tests against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, EventId, EventSendState, RoomId, RoomSummary,
    SubscriptionId, TimelineItemContent, TimelineItemDto, UserId, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

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
        password: "testpassword123!".to_string(),
    })
    .await
    .unwrap();
    (dir, ctx, eng)
}

fn apply_timeline(list: &mut Vec<TimelineItemDto>, diff: VectorDiffDto<TimelineItemDto>) {
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
        VectorDiffDto::Insert { index, value } => {
            if index <= list.len() {
                list.insert(index, value);
            } else {
                list.push(value);
            }
        }
        VectorDiffDto::Set { index, value } => {
            if index < list.len() {
                list[index] = value;
            } else {
                list.push(value);
            }
        }
        VectorDiffDto::Remove { index } => {
            if index < list.len() {
                list.remove(index);
            }
        }
        VectorDiffDto::Truncate { length } => list.truncate(length),
        VectorDiffDto::Reset { values } => *list = values,
    }
}

async fn until_timeline(
    rx: &mut Receiver<EngineEvent>,
    sub: &SubscriptionId,
    mirror: &mut Vec<TimelineItemDto>,
    what: &str,
    done: impl Fn(&[TimelineItemDto]) -> bool,
) {
    let res = tokio::time::timeout(Duration::from_secs(150), async {
        while !done(mirror) {
            if let Ok(event) = rx.recv().await {
                if let EngineEvent::TimelineDiff {
                    subscription_id,
                    diff,
                    ..
                } = event
                {
                    if &subscription_id == sub {
                        apply_timeline(mirror, *diff);
                    }
                }
            }
        }
    })
    .await;
    assert!(
        res.is_ok(),
        "timed out waiting for: {what}; mirror={mirror:#?}"
    );
}

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
        VectorDiffDto::Insert { index, value } => {
            if index <= list.len() {
                list.insert(index, value);
            } else {
                list.push(value);
            }
        }
        VectorDiffDto::Set { index, value } => {
            if index < list.len() {
                list[index] = value;
            } else {
                list.push(value);
            }
        }
        VectorDiffDto::Remove { index } => {
            if index < list.len() {
                list.remove(index);
            }
        }
        VectorDiffDto::Truncate { length } => list.truncate(length),
        VectorDiffDto::Reset { values } => *list = values,
    }
}

async fn pump_rooms_until(
    rx: &mut Receiver<EngineEvent>,
    sub: &SubscriptionId,
    rooms: &mut Vec<RoomSummary>,
    what: &str,
    done: impl Fn(&[RoomSummary]) -> bool,
) {
    let res = tokio::time::timeout(Duration::from_secs(60), async {
        while !done(rooms) {
            if let Ok(event) = rx.recv().await {
                if let EngineEvent::RoomListDiff {
                    subscription_id,
                    diff,
                } = event
                {
                    if &subscription_id == sub {
                        apply_room(rooms, diff);
                    }
                }
            }
        }
    })
    .await;
    assert!(res.is_ok(), "timed out waiting for: {what}");
}

fn text_of(i: &TimelineItemDto) -> Option<&str> {
    match &i.content {
        TimelineItemContent::TextMessage { body, .. } => Some(body),
        _ => None,
    }
}

fn find_item<'a>(m: &'a [TimelineItemDto], body: &str) -> Option<&'a TimelineItemDto> {
    m.iter().find(|i| text_of(i) == Some(body))
}

async fn send_msg(eng: &AppEngine, room_id: &RoomId, body: &str) -> String {
    match eng
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: body.to_string(),
            markdown: Some(false),
            reply_to: None,
            mentions: None,
            mention_room: false,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::MessageSent { txn_id, .. } => txn_id,
        other => panic!("unexpected {other:?}"),
    }
}

async fn send_reply(eng: &AppEngine, room_id: &RoomId, parent_id: &EventId, body: &str) -> String {
    match eng
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: body.to_string(),
            markdown: Some(false),
            reply_to: Some(parent_id.clone()),
            mentions: None,
            mention_room: false,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::MessageSent { txn_id, .. } => txn_id,
        other => panic!("unexpected {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. SetEventPinned + GetPinnedEvents + RoomDetails.pinned_event_ids / can_pin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pin_and_unpin_event_details_and_events() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir, ctx, eng) = login(&hs, "alice").await;

    let room_id = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("pin-test-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let mut events = ctx.sink.subscribe();
    let sub = match eng
        .handle(EngineCommand::SubscribeTimeline {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    send_msg(&eng, &room_id, "pin me please").await;

    let mut mirror = Vec::new();
    until_timeline(
        &mut events,
        &sub,
        &mut mirror,
        "message sent and confirmed",
        |m| {
            find_item(m, "pin me please").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;

    let event_id = find_item(&mirror, "pin me please")
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    // Pin the message
    let pinned_resp = eng
        .handle(EngineCommand::SetEventPinned {
            room_id: room_id.clone(),
            event_id: event_id.clone(),
            pinned: true,
        })
        .await
        .unwrap();

    assert!(matches!(
        pinned_resp,
        EngineResponse::EventPinned { pinned: true, .. }
    ));

    // PinnedEventsChanged event must be emitted
    let mut got_pinned_event = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        if let Ok(event) = tokio::time::timeout(Duration::from_millis(500), events.recv()).await {
            if let Ok(EngineEvent::PinnedEventsChanged {
                room_id: rid,
                event_ids,
            }) = event
            {
                if rid == room_id && event_ids.contains(&event_id) {
                    got_pinned_event = true;
                    break;
                }
            }
        }
    }
    assert!(
        got_pinned_event,
        "PinnedEventsChanged event was not received"
    );

    // GetPinnedEvents returns the pinned message with body
    let pinned_list = match eng
        .handle(EngineCommand::GetPinnedEvents {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::PinnedEvents { items, .. } => items,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(pinned_list.len(), 1);
    assert_eq!(pinned_list[0].event_id, Some(event_id.clone()));
    assert_eq!(text_of(&pinned_list[0]), Some("pin me please"));

    // RoomDetails shows pinned_event_ids
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
    assert!(
        details.pinned_event_ids.contains(&event_id),
        "RoomDetails must include pinned event id, got {:?}",
        details.pinned_event_ids
    );

    // Unpin the message
    let unpinned_resp = eng
        .handle(EngineCommand::SetEventPinned {
            room_id: room_id.clone(),
            event_id: event_id.clone(),
            pinned: false,
        })
        .await
        .unwrap();

    assert!(matches!(
        unpinned_resp,
        EngineResponse::EventPinned { pinned: false, .. }
    ));

    // GetPinnedEvents is now empty
    let unpinned_list = match eng
        .handle(EngineCommand::GetPinnedEvents {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::PinnedEvents { items, .. } => items,
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        unpinned_list.is_empty(),
        "PinnedEvents must be empty after unpin"
    );

    // RoomDetails pinned_event_ids is empty
    let details_after = match eng
        .handle(EngineCommand::GetRoomDetails {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomDetailsRetrieved { details } => details,
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        details_after.pinned_event_ids.is_empty(),
        "RoomDetails pinned_event_ids must be empty after unpin, got {:?}",
        details_after.pinned_event_ids
    );
}

#[tokio::test]
async fn can_pin_power_levels_creator_vs_regular_member() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir_a, _ctx_a, alice) = login(&hs, "alice").await;
    let (_dir_b, _ctx_b, bob) = login(&hs, "bob").await;

    let room_id = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("pin-perm-{}", uuid_like()),
            encrypted: false,
            invite: Some(vec![UserId("@bob:localhost".into())]),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Bob joins the room
    bob.handle(EngineCommand::JoinRoom {
        room_id_or_alias: room_id.0.clone(),
    })
    .await
    .unwrap();

    // Start sync on both so state reaches stores
    alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    bob.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    // Wait until bob sees himself in the room members with power level 0
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(EngineResponse::RoomMembers { members, .. }) = bob
                .handle(EngineCommand::GetRoomMembers {
                    room_id: room_id.clone(),
                })
                .await
            {
                if members.iter().any(|m| m.user_id.0 == "@bob:localhost") {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    })
    .await;

    // Alice (creator, PL 100) has can_pin == true
    let alice_details = match alice
        .handle(EngineCommand::GetRoomDetails {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomDetailsRetrieved { details } => details,
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        alice_details.can_pin,
        "Creator alice must have can_pin == true"
    );

    // Bob (regular member, PL 0) has can_pin == false
    let bob_details = match bob
        .handle(EngineCommand::GetRoomDetails {
            room_id: room_id.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomDetailsRetrieved { details } => details,
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        !bob_details.can_pin,
        "Regular member bob must have can_pin == false"
    );
}

// ---------------------------------------------------------------------------
// 2. ForwardEvents
// ---------------------------------------------------------------------------

#[tokio::test]
async fn forward_events_text_and_reply_and_strip_quotes() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir, ctx, eng) = login(&hs, "alice").await;

    let room_a = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("fwd-a-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let room_b = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("fwd-b-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let mut events = ctx.sink.subscribe();

    let sub_a = match eng
        .handle(EngineCommand::SubscribeTimeline {
            room_id: room_a.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    let sub_b = match eng
        .handle(EngineCommand::SubscribeTimeline {
            room_id: room_b.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    // 1. Send text in room A
    send_msg(&eng, &room_a, "first original message").await;

    let mut mirror_a = Vec::new();
    until_timeline(
        &mut events,
        &sub_a,
        &mut mirror_a,
        "first message confirmed",
        |m| {
            find_item(m, "first original message").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;

    let first_id = find_item(&mirror_a, "first original message")
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    // 2. Send reply in room A
    send_reply(&eng, &room_a, &first_id, "replying to first").await;

    until_timeline(&mut events, &sub_a, &mut mirror_a, "reply confirmed", |m| {
        find_item(m, "replying to first").is_some_and(|i| {
            matches!(i.send_state, EventSendState::Sent { .. })
                && i.event_id.is_some()
                && i.reply_to.is_some()
        })
    })
    .await;

    let reply_id = find_item(&mirror_a, "replying to first")
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    // 3. Forward both to room B
    let forwarded = eng
        .handle(EngineCommand::ForwardEvents {
            room_id: room_a.clone(),
            event_ids: vec![first_id, reply_id],
            targets: vec![room_b.clone()],
        })
        .await
        .unwrap();

    let (fwd_targets, failed) = match forwarded {
        EngineResponse::EventsForwarded { forwarded, failed } => (forwarded, failed),
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(fwd_targets, vec![room_b.clone()]);
    assert!(failed.is_empty(), "expected no failures, got: {failed:?}");

    // 4. Verify in room B's timeline: both bodies appear in order, without quote or reply relation
    let mut mirror_b = Vec::new();
    until_timeline(
        &mut events,
        &sub_b,
        &mut mirror_b,
        "both messages forwarded to B",
        |m| {
            find_item(m, "first original message").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            }) && find_item(m, "replying to first").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;

    let _b_first = find_item(&mirror_b, "first original message").unwrap();
    let b_reply = find_item(&mirror_b, "replying to first").unwrap();

    // Forwarded reply must have no reply_to relation
    assert!(
        b_reply.reply_to.is_none(),
        "Forwarded reply must have no reply relation"
    );
    // Body must not have quote prefix
    assert_eq!(text_of(b_reply), Some("replying to first"));

    // Verify ordering: first message comes before reply
    let pos_first = mirror_b
        .iter()
        .position(|i| text_of(i) == Some("first original message"))
        .unwrap();
    let pos_reply = mirror_b
        .iter()
        .position(|i| text_of(i) == Some("replying to first"))
        .unwrap();
    assert!(
        pos_first < pos_reply,
        "First message must appear before the reply in room B"
    );
}

#[tokio::test]
async fn forward_poll_fails_and_partial_failure() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir, ctx, eng) = login(&hs, "alice").await;

    let room_a = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("fwd-poll-a-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let room_b = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("fwd-poll-b-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    // Send a poll in room A
    let poll_resp = eng
        .handle(EngineCommand::SendPoll {
            room_id: room_a.clone(),
            question: "Forward me?".to_string(),
            answers: vec!["Yes".into(), "No".into()],
            disclosed: true,
            max_selections: 1,
        })
        .await
        .unwrap();

    let poll_id = match poll_resp {
        EngineResponse::MessageSent {
            event_id: Some(id), ..
        } => id,
        other => panic!("unexpected {other:?}"),
    };

    // Forwarding the poll yields a failed entry with an error string
    let fwd_poll = eng
        .handle(EngineCommand::ForwardEvents {
            room_id: room_a.clone(),
            event_ids: vec![poll_id],
            targets: vec![room_b.clone()],
        })
        .await
        .unwrap();

    match fwd_poll {
        EngineResponse::EventsForwarded { forwarded, failed } => {
            assert!(forwarded.is_empty(), "Poll forward should not succeed");
            assert_eq!(failed.len(), 1, "Expected 1 failure entry");
            assert_eq!(failed[0].room_id, room_b);
            assert!(
                !failed[0].error.is_empty(),
                "Expected non-empty error message"
            );
        }
        other => panic!("unexpected {other:?}"),
    }

    // Partial failure: forward a normal message to [room_b, invalid_room]
    let mut events = ctx.sink.subscribe();
    let sub_a = match eng
        .handle(EngineCommand::SubscribeTimeline {
            room_id: room_a.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    send_msg(&eng, &room_a, "message for partial forward").await;

    let mut mirror_a = Vec::new();
    until_timeline(
        &mut events,
        &sub_a,
        &mut mirror_a,
        "partial message confirmed",
        |m| {
            find_item(m, "message for partial forward").is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;

    let msg_id = find_item(&mirror_a, "message for partial forward")
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    let bad_room = RoomId("!nonexistent:localhost".into());
    let partial_fwd = eng
        .handle(EngineCommand::ForwardEvents {
            room_id: room_a.clone(),
            event_ids: vec![msg_id],
            targets: vec![room_b.clone(), bad_room.clone()],
        })
        .await
        .unwrap();

    match partial_fwd {
        EngineResponse::EventsForwarded { forwarded, failed } => {
            assert_eq!(
                forwarded,
                vec![room_b.clone()],
                "Valid target room_b should succeed"
            );
            assert_eq!(failed.len(), 1, "Invalid room should fail");
            assert_eq!(failed[0].room_id, bad_room);
            assert!(
                !failed[0].error.is_empty(),
                "Failure should contain error message"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 3. SetRoomFavourite, SetRoomLowPriority, SetRoomMarkedUnread
// ---------------------------------------------------------------------------

#[tokio::test]
async fn room_list_summary_favourite_low_priority_marked_unread() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir, ctx, eng) = login(&hs, "alice").await;

    let mut events = ctx.sink.subscribe();
    let sub = match eng
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };

    let room_id = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("flags-test-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let mut rooms = Vec::new();
    pump_rooms_until(&mut events, &sub, &mut rooms, "room appears in list", |r| {
        r.iter().any(|room| room.id == room_id)
    })
    .await;

    // 1. Set favourite = true
    let resp = eng
        .handle(EngineCommand::SetRoomFavourite {
            room_id: room_id.clone(),
            favourite: true,
        })
        .await
        .unwrap();
    assert!(matches!(
        resp,
        EngineResponse::RoomFavouriteSet {
            favourite: true,
            ..
        }
    ));

    pump_rooms_until(
        &mut events,
        &sub,
        &mut rooms,
        "room becomes favourite",
        |r| {
            r.iter()
                .find(|room| room.id == room_id)
                .is_some_and(|room| room.favourite == Some(true))
        },
    )
    .await;

    // Flip favourite = false
    eng.handle(EngineCommand::SetRoomFavourite {
        room_id: room_id.clone(),
        favourite: false,
    })
    .await
    .unwrap();

    pump_rooms_until(&mut events, &sub, &mut rooms, "room unfavourited", |r| {
        r.iter()
            .find(|room| room.id == room_id)
            .is_some_and(|room| room.favourite == Some(false))
    })
    .await;

    // 2. Set low priority (archive) = true
    let resp = eng
        .handle(EngineCommand::SetRoomLowPriority {
            room_id: room_id.clone(),
            low_priority: true,
        })
        .await
        .unwrap();
    assert!(matches!(
        resp,
        EngineResponse::RoomLowPrioritySet {
            low_priority: true,
            ..
        }
    ));

    pump_rooms_until(
        &mut events,
        &sub,
        &mut rooms,
        "room low_priority set",
        |r| {
            r.iter()
                .find(|room| room.id == room_id)
                .is_some_and(|room| room.low_priority == Some(true))
        },
    )
    .await;

    // Flip low priority = false
    eng.handle(EngineCommand::SetRoomLowPriority {
        room_id: room_id.clone(),
        low_priority: false,
    })
    .await
    .unwrap();

    pump_rooms_until(
        &mut events,
        &sub,
        &mut rooms,
        "room low_priority unset",
        |r| {
            r.iter()
                .find(|room| room.id == room_id)
                .is_some_and(|room| room.low_priority == Some(false))
        },
    )
    .await;

    // 3. Set marked unread = true
    let resp = eng
        .handle(EngineCommand::SetRoomMarkedUnread {
            room_id: room_id.clone(),
            unread: true,
        })
        .await
        .unwrap();
    assert!(matches!(
        resp,
        EngineResponse::RoomMarkedUnreadSet { unread: true, .. }
    ));

    pump_rooms_until(
        &mut events,
        &sub,
        &mut rooms,
        "room marked unread set",
        |r| {
            r.iter()
                .find(|room| room.id == room_id)
                .is_some_and(|room| room.marked_unread)
        },
    )
    .await;

    // Flip marked unread = false
    eng.handle(EngineCommand::SetRoomMarkedUnread {
        room_id: room_id.clone(),
        unread: false,
    })
    .await
    .unwrap();

    pump_rooms_until(
        &mut events,
        &sub,
        &mut rooms,
        "room marked unread unset",
        |r| {
            r.iter()
                .find(|room| room.id == room_id)
                .is_some_and(|room| !room.marked_unread)
        },
    )
    .await;
}

// ---------------------------------------------------------------------------
// 4. Drafts: SaveDraft, GetDraft, ListDrafts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn save_get_and_list_drafts() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let (_dir, _ctx, eng) = login(&hs, "alice").await;

    let room_id = match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("drafts-{}", uuid_like()),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };

    let dummy_reply = EventId("$dummy_event_id:localhost".into());

    // 1. Save draft with reply_to
    let saved = eng
        .handle(EngineCommand::SaveDraft {
            room_id: room_id.clone(),
            text: "Hello future world".to_string(),
            reply_to: Some(dummy_reply.clone()),
            edit_of: None,
        })
        .await
        .unwrap();
    assert!(matches!(saved, EngineResponse::DraftSaved { .. }));

    // 2. GetDraft returns text and reply_to
    let draft_resp = eng
        .handle(EngineCommand::GetDraft {
            room_id: room_id.clone(),
        })
        .await
        .unwrap();

    match draft_resp {
        EngineResponse::Draft {
            room_id: rid,
            draft: Some(dto),
        } => {
            assert_eq!(rid, room_id);
            assert_eq!(dto.text, "Hello future world");
            assert_eq!(dto.reply_to, Some(dummy_reply));
            assert_eq!(dto.edit_of, None);
        }
        other => panic!("unexpected draft response: {other:?}"),
    }

    // 3. ListDrafts includes room
    let list_resp = eng.handle(EngineCommand::ListDrafts).await.unwrap();
    match list_resp {
        EngineResponse::Drafts { items } => {
            let found = items.iter().find(|d| d.room_id == room_id);
            assert!(
                found.is_some(),
                "Expected room in ListDrafts, items: {items:?}"
            );
            assert_eq!(found.unwrap().text, "Hello future world");
        }
        other => panic!("unexpected {other:?}"),
    }

    // 4. Save empty text clears draft
    eng.handle(EngineCommand::SaveDraft {
        room_id: room_id.clone(),
        text: "".to_string(),
        reply_to: None,
        edit_of: None,
    })
    .await
    .unwrap();

    // GetDraft -> None
    let draft_cleared = eng
        .handle(EngineCommand::GetDraft {
            room_id: room_id.clone(),
        })
        .await
        .unwrap();
    match draft_cleared {
        EngineResponse::Draft { draft: None, .. } => {}
        other => panic!("expected cleared draft, got: {other:?}"),
    }

    // ListDrafts does not include room
    let list_cleared = eng.handle(EngineCommand::ListDrafts).await.unwrap();
    match list_cleared {
        EngineResponse::Drafts { items } => {
            assert!(
                !items.iter().any(|d| d.room_id == room_id),
                "Cleared room draft must not appear in ListDrafts"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}
