//! Room tags against a local Synapse: the tag catalogue round trip and its validation, and
//! assigning, clearing and deleting custom `u.<id>` tags on rooms. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, RoomId, RoomSummary, RoomTagDef, SubscriptionId,
    TagColour, VectorDiffDto,
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

fn unique_id(prefix: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &suffix[..8])
}

async fn unit(eng: &AppEngine, cmd: EngineCommand) {
    match eng.handle(cmd).await.unwrap() {
        EngineResponse::Unit => {}
        other => panic!("unexpected {other:?}"),
    }
}

async fn invalid(eng: &AppEngine, cmd: EngineCommand) {
    let err = eng.handle(cmd).await.unwrap_err();
    assert_eq!(err.code, "invalid_argument", "message: {}", err.message);
}

async fn catalogue(eng: &AppEngine) -> Vec<RoomTagDef> {
    match eng.handle(EngineCommand::GetTagCatalog).await.unwrap() {
        EngineResponse::TagCatalog { tags } => tags,
        other => panic!("unexpected {other:?}"),
    }
}

async fn create_room(eng: &AppEngine) -> RoomId {
    match eng
        .handle(EngineCommand::CreateRoom {
            name: format!("tags-test-{}", uuid::Uuid::new_v4().simple()),
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

/// Apply room-list diffs to `rooms` until `done` holds.
async fn pump_rooms_until(
    rx: &mut Receiver<EngineEvent>,
    sub: &SubscriptionId,
    rooms: &mut Vec<RoomSummary>,
    what: &str,
    done: impl Fn(&[RoomSummary]) -> bool,
) {
    let res = tokio::time::timeout(Duration::from_secs(60), async {
        while !done(rooms) {
            match rx.recv().await.unwrap() {
                EngineEvent::RoomListDiff {
                    subscription_id,
                    diff,
                } if &subscription_id == sub => apply_room(rooms, diff),
                _ => {}
            }
        }
    })
    .await;
    assert!(res.is_ok(), "timed out waiting for: {what}");
}

fn tags_of(rooms: &[RoomSummary], room_id: &RoomId) -> Vec<String> {
    rooms
        .iter()
        .find(|r| &r.id == room_id)
        .map(|r| r.tags.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn tag_catalogue_round_trips_and_rejects_invalid_entries() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_dir, _ctx, bob) = login(&hs, "bob").await;

    let work = unique_id("work");
    let home = unique_id("home");
    unit(
        &bob,
        EngineCommand::SaveTagCatalog {
            tags: vec![
                RoomTagDef {
                    id: work.clone(),
                    name: "  Work  ".into(),
                    colour: TagColour::Blue,
                },
                RoomTagDef {
                    id: home.clone(),
                    name: "Home".into(),
                    colour: TagColour::Teal,
                },
            ],
        },
    )
    .await;
    // Names are trimmed on save; order is kept.
    assert_eq!(
        catalogue(&bob).await,
        vec![
            RoomTagDef {
                id: work.clone(),
                name: "Work".into(),
                colour: TagColour::Blue,
            },
            RoomTagDef {
                id: home.clone(),
                name: "Home".into(),
                colour: TagColour::Teal,
            },
        ]
    );

    let def = |id: &str, name: &str| RoomTagDef {
        id: id.into(),
        name: name.into(),
        colour: TagColour::Grey,
    };
    for tags in [
        vec![def("Bad Id", "Bad")],
        vec![def("", "Empty id")],
        vec![def(&"x".repeat(33), "Too long")],
        vec![def(&unique_id("blank"), "   ")],
        vec![def(&unique_id("long"), &"n".repeat(33))],
        vec![def(&work, "Work"), def(&work, "Work again")],
    ] {
        invalid(&bob, EngineCommand::SaveTagCatalog { tags }).await;
    }
    invalid(
        &bob,
        EngineCommand::SetRoomTag {
            room_id: RoomId("!unknown:localhost".into()),
            tag_id: "UPPER".into(),
            enabled: true,
        },
    )
    .await;
    invalid(
        &bob,
        EngineCommand::DeleteRoomTag {
            tag_id: "Bad Id".into(),
        },
    )
    .await;

    // Rejected saves leave the stored catalogue untouched.
    assert_eq!(catalogue(&bob).await.len(), 2);
    assert_eq!(catalogue(&bob).await[0].id, work);
}

#[tokio::test]
async fn room_tags_are_assigned_per_room_and_deleted_everywhere() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let (_dir, ctx, alice) = login(&hs, "alice").await;
    let room1 = create_room(&alice).await;
    let room2 = create_room(&alice).await;

    let mut rx = ctx.sink.subscribe();
    let sub = match alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };
    let mut rooms: Vec<RoomSummary> = vec![];
    let ids = [room1.clone(), room2.clone()];
    pump_rooms_until(&mut rx, &sub, &mut rooms, "both rooms listed", |rooms| {
        ids.iter().all(|id| rooms.iter().any(|r| &r.id == id))
    })
    .await;

    let tag = unique_id("project");
    let orphan = unique_id("orphan");
    unit(
        &alice,
        EngineCommand::SaveTagCatalog {
            tags: vec![RoomTagDef {
                id: tag.clone(),
                name: "Project".into(),
                colour: TagColour::Violet,
            }],
        },
    )
    .await;

    // Assign on room 1 only.
    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room1.clone(),
            tag_id: tag.clone(),
            enabled: true,
        },
    )
    .await;
    pump_rooms_until(&mut rx, &sub, &mut rooms, "tag on room 1", |rooms| {
        tags_of(rooms, &room1).contains(&tag)
    })
    .await;
    assert!(tags_of(&rooms, &room2).is_empty());

    // A tag with no catalogue entry still shows on the room, and is not added to the catalogue.
    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room1.clone(),
            tag_id: orphan.clone(),
            enabled: true,
        },
    )
    .await;
    pump_rooms_until(&mut rx, &sub, &mut rooms, "orphan tag on room 1", |rooms| {
        tags_of(rooms, &room1).contains(&orphan)
    })
    .await;
    assert!(tags_of(&rooms, &room1).contains(&tag));
    assert_eq!(
        catalogue(&alice)
            .await
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec![tag.as_str()]
    );

    // Assign and clear on room 2; clearing is idempotent.
    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room2.clone(),
            tag_id: tag.clone(),
            enabled: true,
        },
    )
    .await;
    pump_rooms_until(&mut rx, &sub, &mut rooms, "tag on room 2", |rooms| {
        tags_of(rooms, &room2).contains(&tag)
    })
    .await;
    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room2.clone(),
            tag_id: tag.clone(),
            enabled: false,
        },
    )
    .await;
    pump_rooms_until(
        &mut rx,
        &sub,
        &mut rooms,
        "tag cleared on room 2",
        |rooms| !tags_of(rooms, &room2).contains(&tag),
    )
    .await;
    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room2.clone(),
            tag_id: tag.clone(),
            enabled: false,
        },
    )
    .await;

    // Deleting the tag removes it from every room and from the catalogue.
    unit(
        &alice,
        EngineCommand::DeleteRoomTag {
            tag_id: tag.clone(),
        },
    )
    .await;
    pump_rooms_until(
        &mut rx,
        &sub,
        &mut rooms,
        "tag deleted from room 1",
        |rooms| !tags_of(rooms, &room1).contains(&tag),
    )
    .await;
    assert!(tags_of(&rooms, &room1).contains(&orphan));
    assert!(tags_of(&rooms, &room2).is_empty());
    assert!(catalogue(&alice).await.is_empty());

    // Deleting again is a no-op.
    unit(
        &alice,
        EngineCommand::DeleteRoomTag {
            tag_id: tag.clone(),
        },
    )
    .await;
    assert!(catalogue(&alice).await.is_empty());
}
