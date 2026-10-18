//! Poll lifecycle against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008.
//! Accounts alice and bob use password `testpassword123!`.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, EventSendState, RoomId, SubscriptionId,
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

fn poll_of<'a>(m: &'a [TimelineItemDto], question: &str) -> Option<&'a TimelineItemDto> {
    m.iter().find(|i| {
        matches!(
            &i.content,
            TimelineItemContent::Poll { question: q, .. } if q == question
        )
    })
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

#[tokio::test]
async fn poll_vote_and_end_reach_creator_timeline() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_alice_dir, alice_ctx, alice) = login(&hs, "alice").await;
    let (_bob_dir, _bob_ctx, bob) = login(&hs, "bob").await;

    // Alice creates a room, invites bob, and bob joins.
    let room_id: RoomId = match alice
        .handle(EngineCommand::CreateRoom {
            name: format!("poll-test-{}", uuid::Uuid::new_v4().simple()),
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

    let mut alice_rx = alice_ctx.sink.subscribe();
    let alice_sub = subscribe_timeline(&alice, &room_id).await;
    // Bob needs an open timeline to send a response into the room.
    let _bob_sub = subscribe_timeline(&bob, &room_id).await;
    let mut mirrors: Vec<Vec<TimelineItemDto>> = vec![vec![]];

    // Alice creates a disclosed single-choice poll and waits for the server echo.
    let question = "Lunch?";
    let sent = alice
        .handle(EngineCommand::SendPoll {
            room_id: room_id.clone(),
            question: question.into(),
            answers: vec!["Pizza".into(), "Sushi".into(), "Salad".into()],
            disclosed: true,
            max_selections: 1,
        })
        .await
        .unwrap();
    assert!(
        matches!(sent, EngineResponse::MessageSent { .. }),
        "unexpected {sent:?}"
    );
    until(
        &mut alice_rx,
        &[&alice_sub],
        &mut mirrors,
        "poll sent",
        |m| {
            poll_of(&m[0], question).is_some_and(|i| {
                matches!(i.send_state, EventSendState::Sent { .. }) && i.event_id.is_some()
            })
        },
    )
    .await;
    let poll_id = poll_of(&mirrors[0], question)
        .unwrap()
        .event_id
        .clone()
        .unwrap();

    // Bob votes for "Sushi" (answer id "1").
    let responded = bob
        .handle(EngineCommand::RespondToPoll {
            room_id: room_id.clone(),
            poll_event_id: poll_id.clone(),
            answer_ids: vec!["1".into()],
        })
        .await
        .unwrap();
    assert!(
        matches!(responded, EngineResponse::Unit),
        "unexpected {responded:?}"
    );

    // Alice sees the vote in the disclosed counts; she has not voted herself.
    until(
        &mut alice_rx,
        &[&alice_sub],
        &mut mirrors,
        "bob's vote counted",
        |m| {
            poll_of(&m[0], question).is_some_and(|i| {
                matches!(
                    &i.content,
                    TimelineItemContent::Poll { answers, total_votes, my_votes, .. }
                        if *total_votes == 1
                            && answers[1].votes == 1
                            && answers[0].votes == 0
                            && my_votes.is_empty()
                )
            })
        },
    )
    .await;

    // Alice, the creator, ends the poll.
    let ended = alice
        .handle(EngineCommand::EndPoll {
            room_id: room_id.clone(),
            poll_event_id: poll_id.clone(),
        })
        .await
        .unwrap();
    assert!(
        matches!(ended, EngineResponse::Unit),
        "unexpected {ended:?}"
    );

    until(
        &mut alice_rx,
        &[&alice_sub],
        &mut mirrors,
        "poll ended",
        |m| {
            poll_of(&m[0], question).is_some_and(|i| {
                matches!(
                    &i.content,
                    TimelineItemContent::Poll {
                        has_ended: true,
                        ..
                    }
                )
            })
        },
    )
    .await;
}
