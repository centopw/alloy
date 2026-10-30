//! Timeline handler against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, EventId, EventSendState, RoomId, SubscriptionId,
    TimelineItemContent, TimelineItemDto, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

/// Both tests drive the same account and its sync service from parallel threads. When they start
/// together the timeline of one can sit for minutes before its first event, so they run one at a
/// time: this is about the fixture sharing an account, not about the product.
static ACCOUNT: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Upper bound for one event wait. The waits are event-driven; this only catches a hang, so it is
/// well above the observed runtime instead of just above it.
const WAIT_TIMEOUT: Duration = Duration::from_secs(180);

/// Wall-clock bound for a whole test. The SDK client sets no request timeout, so a stalled HTTP
/// call can await forever; this turns that into a named failure in minutes instead of a shard that
/// sits until the job is cancelled with no log.
const TEST_TIMEOUT: Duration = Duration::from_secs(300);

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

/// Pump events into `mirror` until `done(mirror)` holds or the timeout hits.
async fn until(
    rx: &mut Receiver<EngineEvent>,
    sub: &SubscriptionId,
    mirror: &mut Vec<TimelineItemDto>,
    what: &str,
    done: impl Fn(&[TimelineItemDto]) -> bool,
) {
    let res = tokio::time::timeout(WAIT_TIMEOUT, async {
        while !done(mirror) {
            if let EngineEvent::TimelineDiff {
                subscription_id,
                diff,
                ..
            } = rx.recv().await.unwrap()
                && &subscription_id == sub
            {
                apply(mirror, *diff);
            }
        }
    })
    .await;
    assert!(
        res.is_ok(),
        "timed out waiting for: {what}; mirror={mirror:#?}"
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

#[tokio::test]
async fn send_edit_react_redact_and_paginate() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let _guard = ACCOUNT.lock().await;
    let finished =
        tokio::time::timeout(TEST_TIMEOUT, body_send_edit_react_redact_and_paginate(hs)).await;
    assert!(
        finished.is_ok(),
        "the test did not finish within {TEST_TIMEOUT:?}"
    );
}

async fn body_send_edit_react_redact_and_paginate(hs: String) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs,
        username: "alice".into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();

    let room_id: RoomId = match eng
        .handle(EngineCommand::CreateRoom {
            name: "timeline-test".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    // The room has to be known to the client (sync) before its timeline can be opened.
    eng.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
    let mut rx = ctx.sink.subscribe();
    let mut attempts = 0;
    let sub = loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room_id.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => break subscription_id,
            Err(e) if e.code == "not_found" && attempts < 30 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    };
    let mut mirror: Vec<TimelineItemDto> = vec![];

    // Send: local echo first (Sending, with txn id), then the remote event (Sent).
    let txn = match eng
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: "hello **world**".into(),
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
    };
    assert!(!txn.is_empty());
    until(&mut rx, &sub, &mut mirror, "message sent", |m| {
        find(m, "hello **world**").is_some_and(|i| {
            matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
        })
    })
    .await;
    let first = find(&mirror, "hello **world**").unwrap().clone();
    assert!(first.is_own);
    let first_id: EventId = first.event_id.clone().unwrap();

    // Reply carries the quoted parent.
    eng.handle(EngineCommand::SendText {
        room_id: room_id.clone(),
        body: "a reply".into(),
        markdown: Some(false),
        reply_to: Some(first_id.clone()),
        mentions: None,
        mention_room: false,
        txn_id: None,
        in_thread_root: None,
    })
    .await
    .unwrap();
    until(&mut rx, &sub, &mut mirror, "reply with parent", |m| {
        find(m, "a reply").is_some_and(|i| i.reply_to.is_some())
    })
    .await;

    // Edit.
    eng.handle(EngineCommand::EditMessage {
        room_id: room_id.clone(),
        event_id: first_id.clone(),
        body: "edited text".into(),
        markdown: Some(false),
    })
    .await
    .unwrap();
    until(&mut rx, &sub, &mut mirror, "edit applied", |m| {
        find(m, "edited text").is_some_and(|i| i.edited)
    })
    .await;

    // React, aggregated with by_me.
    match eng
        .handle(EngineCommand::ToggleReaction {
            room_id: room_id.clone(),
            event_id: first_id.clone(),
            key: "+1".into(),
        })
        .await
        .unwrap()
    {
        EngineResponse::ReactionToggled { added, .. } => assert!(added),
        other => panic!("unexpected {other:?}"),
    }
    until(&mut rx, &sub, &mut mirror, "reaction", |m| {
        find(m, "edited text").is_some_and(|i| {
            i.reactions
                .iter()
                .any(|r| r.key == "+1" && r.count == 1 && r.by_me)
        })
    })
    .await;

    // Redact.
    eng.handle(EngineCommand::RedactEvent {
        room_id: room_id.clone(),
        event_id: first_id.clone(),
        reason: None,
    })
    .await
    .unwrap();
    until(&mut rx, &sub, &mut mirror, "redaction", |m| {
        m.iter().any(|i| {
            i.event_id.as_ref() == Some(&first_id)
                && matches!(i.content, TimelineItemContent::Redacted)
        })
    })
    .await;

    // Read receipt + typing + pagination reach the SDK without error.
    let last = mirror
        .iter()
        .rev()
        .find_map(|i| i.event_id.clone())
        .unwrap();
    eng.handle(EngineCommand::MarkRead {
        room_id: room_id.clone(),
        event_id: last,
    })
    .await
    .unwrap();
    eng.handle(EngineCommand::SetTyping {
        room_id: room_id.clone(),
        typing: true,
    })
    .await
    .unwrap();
    let mut reached = false;
    for _ in 0..10 {
        match eng
            .handle(EngineCommand::PaginateBackwards {
                room_id: room_id.clone(),
                count: 20,
            })
            .await
            .unwrap()
        {
            EngineResponse::Paginated { reached_start, .. } => {
                if reached_start {
                    reached = true;
                    break;
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(reached, "never reached the start of the room");

    // Empty messages and unknown rooms are typed errors.
    let err = eng
        .handle(EngineCommand::SendText {
            room_id: room_id.clone(),
            body: "  ".into(),
            markdown: None,
            reply_to: None,
            mentions: None,
            mention_room: false,
            txn_id: None,
            in_thread_root: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument");
    let err = eng
        .handle(EngineCommand::SubscribeTimeline {
            room_id: RoomId("!nope:localhost".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
}

/// Create a room and return its id.
async fn create_room(eng: &AppEngine, name: &str) -> RoomId {
    match eng
        .handle(EngineCommand::CreateRoom {
            name: name.to_owned(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    }
}

/// Subscribe to a room's timeline, waiting for the room to be known to the client.
async fn subscribe_room(eng: &AppEngine, room: &RoomId) -> SubscriptionId {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        match eng
            .handle(EngineCommand::SubscribeTimeline {
                room_id: room.clone(),
            })
            .await
        {
            Ok(EngineResponse::TimelineSubscribed { subscription_id }) => return subscription_id,
            Err(e) if e.code == "not_found" && tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// Send with a bounded retry for the window where the client still reads the room as invited.
async fn send_text(eng: &AppEngine, room: &RoomId, body: &str) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let result = eng
            .handle(EngineCommand::SendText {
                room_id: room.clone(),
                body: body.to_owned(),
                markdown: Some(false),
                reply_to: None,
                mentions: None,
                mention_room: false,
                txn_id: None,
                in_thread_root: None,
            })
            .await;
        match result {
            Ok(_) => return,
            Err(e)
                if e.code == "network_error"
                    && e.message == "the room isn't in the joined state"
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("send failed: {e:?}"),
        }
    }
}

/// #T10b: closing a timeline aborts that room's subscriptions, drops the cached timeline, and the
/// second call is a no-op. Re-subscribing rebuilds the timeline and replays a full `Reset` that
/// already holds what arrived while it was closed.
#[tokio::test]
async fn closing_a_timeline_stops_its_diffs_and_replays_a_fresh_reset() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let _guard = ACCOUNT.lock().await;
    let finished =
        tokio::time::timeout(TEST_TIMEOUT, body_closing_a_timeline_stops_its_diffs(hs)).await;
    assert!(
        finished.is_ok(),
        "the test did not finish within {TEST_TIMEOUT:?}"
    );
}

async fn body_closing_a_timeline_stops_its_diffs(hs: String) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs,
        username: "alice".into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();

    let room = create_room(&eng, "close-timeline").await;
    let control = create_room(&eng, "close-contro").await;
    eng.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();

    let mut rx = ctx.sink.subscribe();
    let sub = subscribe_room(&eng, &room).await;
    let sub_control = subscribe_room(&eng, &control).await;
    let mut mirror: Vec<TimelineItemDto> = vec![];
    send_text(&eng, &room, "before close").await;
    until(
        &mut rx,
        &sub,
        &mut mirror,
        "the message before closing",
        |m| find(m, "before close").is_some(),
    )
    .await;

    // Close: everything for this room is aborted and dropped.
    match eng
        .handle(EngineCommand::CloseTimeline {
            room_id: room.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineClosed { closed, .. } => assert!(closed, "there was a timeline"),
        other => panic!("unexpected {other:?}"),
    }
    // A second close has nothing to do.
    match eng
        .handle(EngineCommand::CloseTimeline {
            room_id: room.clone(),
        })
        .await
        .unwrap()
    {
        EngineResponse::TimelineClosed { closed, .. } => assert!(!closed, "nothing left to close"),
        other => panic!("unexpected {other:?}"),
    }

    // While the room is closed, a message arrives and the control room keeps streaming: the
    // closed subscription must produce nothing, and the control proves the stream is alive.
    // Cancellation is asynchronous, so let the aborted task finish tearing down and drop whatever
    // it had already emitted; the check below is then about the closed subscription only.
    tokio::time::sleep(Duration::from_millis(300)).await;
    while rx.try_recv().is_ok() {}
    send_text(&eng, &room, "while closed").await;
    send_text(&eng, &control, "control message").await;
    let mut control_mirror: Vec<TimelineItemDto> = vec![];
    let mut closed_sub_diffs = 0;
    let res = tokio::time::timeout(WAIT_TIMEOUT, async {
        while find(&control_mirror, "control message").is_none() {
            match rx.recv().await.unwrap() {
                EngineEvent::TimelineDiff {
                    subscription_id,
                    diff,
                    ..
                } if subscription_id == sub => {
                    closed_sub_diffs += 1;
                    apply(&mut mirror, *diff);
                }
                EngineEvent::TimelineDiff {
                    subscription_id,
                    diff,
                    ..
                } if subscription_id == sub_control => apply(&mut control_mirror, *diff),
                _ => {}
            }
        }
    })
    .await;
    assert!(
        res.is_ok(),
        "the control room's timeline stopped streaming too"
    );
    assert_eq!(
        closed_sub_diffs, 0,
        "a closed timeline must not emit diffs; mirror={mirror:#?}"
    );

    // Re-subscribing rebuilds it: a new id and a full Reset that already holds the closed-window
    // message.
    let reopened = subscribe_room(&eng, &room).await;
    assert_ne!(reopened, sub);
    let mut rebuilt: Vec<TimelineItemDto> = vec![];
    until(
        &mut rx,
        &reopened,
        &mut rebuilt,
        "the rebuilt timeline",
        |m| find(m, "while closed").is_some() && find(m, "before close").is_some(),
    )
    .await;

    // An unknown room is not "already closed", it is a bad request.
    let err = eng
        .handle(EngineCommand::CloseTimeline {
            room_id: RoomId("!nope:localhost".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
}
