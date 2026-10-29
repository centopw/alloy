//! Room tags against a local Synapse: the tag catalogue round trip and its validation, and
//! assigning, clearing and deleting custom `u.<id>` tags on rooms. Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, RoomId, RoomSummary, RoomTagDef, SubscriptionId,
    TagColour, VectorDiffDto,
};
use alloy_storage::InMemorySecretStore;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

const PASSWORD: &str = "testpassword123!";

/// The tag catalogue is a single global account-data event per user, so every test here reads or
/// writes the same value. Tests in one binary run in parallel threads, and a create, rename,
/// reorder or delete in one test changes the catalogue another test is asserting on. Serialize the
/// file's tests so each sees the catalogue it wrote.
static CATALOGUE: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

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

async fn not_found(eng: &AppEngine, cmd: EngineCommand) {
    let err = eng.handle(cmd).await.unwrap_err();
    assert_eq!(err.code, "not_found", "message: {}", err.message);
}

async fn create_tag(eng: &AppEngine, name: &str, colour: TagColour) -> RoomTagDef {
    match eng
        .handle(EngineCommand::CreateRoomTag {
            name: name.to_owned(),
            colour,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomTagCreated { tag } => tag,
        other => panic!("unexpected {other:?}"),
    }
}

/// Remove every tag the account's catalogue holds, so a test starts from a known state.
async fn clear_catalogue(eng: &AppEngine) {
    for tag in catalogue(eng).await {
        unit(eng, EngineCommand::DeleteRoomTag { tag_id: tag.id }).await;
    }
}

fn tag_ids(tags: Vec<RoomTagDef>) -> Vec<String> {
    tags.into_iter().map(|tag| tag.id).collect()
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
    let _guard = CATALOGUE.lock().await;
    let (_dir, _ctx, bob) = login(&hs, "bob").await;
    clear_catalogue(&bob).await;

    let work_name = format!("  Work {}  ", unique_id("w"));
    let work = create_tag(&bob, &work_name, TagColour::Blue).await;
    // The name is trimmed and the id is a slug of it, assigned by the engine.
    assert_eq!(work.name, work_name.trim());
    assert!(
        work.id.starts_with("work-") && !work.id.contains(' '),
        "id: {}",
        work.id
    );
    let home = create_tag(&bob, &format!("Home {}", unique_id("h")), TagColour::Teal).await;
    assert_eq!(
        tag_ids(catalogue(&bob).await),
        vec![work.id.clone(), home.id.clone()],
        "created tags come back in the order they were added"
    );

    for name in ["   ".to_owned(), "n".repeat(33)] {
        invalid(
            &bob,
            EngineCommand::CreateRoomTag {
                name,
                colour: TagColour::Grey,
            },
        )
        .await;
    }
    invalid(
        &bob,
        EngineCommand::RenameRoomTag {
            id: "Bad Id".into(),
            name: Some("Bad".into()),
            colour: None,
        },
    )
    .await;
    invalid(
        &bob,
        EngineCommand::RenameRoomTag {
            id: work.id.clone(),
            name: Some("   ".into()),
            colour: None,
        },
    )
    .await;
    not_found(
        &bob,
        EngineCommand::RenameRoomTag {
            id: unique_id("missing"),
            name: Some("Nowhere".into()),
            colour: None,
        },
    )
    .await;
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

    // Rejected changes leave the stored catalogue untouched.
    assert_eq!(tag_ids(catalogue(&bob).await), vec![work.id, home.id]);
}

#[tokio::test]
async fn room_tags_are_assigned_per_room_and_deleted_everywhere() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let _guard = CATALOGUE.lock().await;
    let (_dir, ctx, alice) = login(&hs, "alice").await;
    clear_catalogue(&alice).await;
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

    let tag = create_tag(&alice, "Project", TagColour::Violet).await.id;
    let orphan = unique_id("orphan");

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

/// #116: the assignments are `u.<id>` room tags and the catalogue is the `chat.alloy.room_tags`
/// account-data event, both stored on the server. A client with an empty local store — a reinstall
/// or a second device — must rebuild the room's tags and the catalogue from a plain sync.
#[tokio::test]
async fn tags_reload_on_a_cold_client() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let _guard = CATALOGUE.lock().await;

    // Device 1 defines a catalogue entry and assigns it to a new room.
    let (_dir1, ctx1, alice) = login(&hs, "alice").await;
    clear_catalogue(&alice).await;
    let room = create_room(&alice).await;
    let tag = create_tag(&alice, &unique_id("cold"), TagColour::Violet)
        .await
        .id;

    let mut rx = ctx1.sink.subscribe();
    let sub = match alice
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };
    let mut rooms: Vec<RoomSummary> = vec![];
    pump_rooms_until(&mut rx, &sub, &mut rooms, "new room listed", |rooms| {
        rooms.iter().any(|r| r.id == room)
    })
    .await;

    unit(
        &alice,
        EngineCommand::SetRoomTag {
            room_id: room.clone(),
            tag_id: tag.clone(),
            enabled: true,
        },
    )
    .await;
    pump_rooms_until(
        &mut rx,
        &sub,
        &mut rooms,
        "tag visible to device 1",
        |rooms| tags_of(rooms, &room).contains(&tag),
    )
    .await;

    // Device 2 has no local state: only what the server holds can show up.
    let (_dir2, ctx2, alice2) = login(&hs, "alice").await;
    let mut rx2 = ctx2.sink.subscribe();
    let sub2 = match alice2
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap()
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected {other:?}"),
    };
    let mut rooms2: Vec<RoomSummary> = vec![];
    pump_rooms_until(
        &mut rx2,
        &sub2,
        &mut rooms2,
        "room with its persisted tag after a fresh login",
        |rooms| tags_of(rooms, &room).contains(&tag),
    )
    .await;

    let catalogue_ids: Vec<String> = catalogue(&alice2).await.into_iter().map(|t| t.id).collect();
    assert!(
        catalogue_ids.contains(&tag),
        "the catalogue must come back from the server, got {catalogue_ids:?}"
    );
}

/// #T28: every command applies one change to the catalogue the server holds, so a device working
/// from a copy that predates another device's tag cannot drop it — the class a whole-list write
/// allowed.
#[tokio::test]
async fn per_operation_commands_keep_another_clients_tags() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let _guard = CATALOGUE.lock().await;

    let run = unique_id("perop");
    let (_dir1, _ctx1, first) = login(&hs, "alice").await;
    let (_dir2, _ctx2, second) = login(&hs, "alice").await;

    // Start from a known catalogue, then give the first device two tags.
    clear_catalogue(&first).await;
    let alpha = create_tag(&first, &format!("Alpha {run}"), TagColour::Violet).await;
    let beta = create_tag(&first, &format!("Beta {run}"), TagColour::Violet).await;
    assert_eq!(
        alpha.id,
        format!("alpha-{run}"),
        "the id is a slug of the name"
    );

    // The second device adds a tag the first device has never seen.
    let gamma = create_tag(&second, &format!("Gamma {run}"), TagColour::Violet).await;

    // A rename from the first device's stale view must not drop it.
    unit(
        &first,
        EngineCommand::RenameRoomTag {
            id: alpha.id.clone(),
            name: Some(format!("Alpha renamed {run}")),
            colour: Some(TagColour::Teal),
        },
    )
    .await;
    let ids = tag_ids(catalogue(&second).await);
    assert!(
        ids.contains(&gamma.id),
        "the other device's tag must survive a rename, got {ids:?}"
    );
    assert!(ids.contains(&beta.id), "and every other entry, got {ids:?}");

    // A reorder built from the stale copy is refused rather than dropping the unknown tag.
    let err = first
        .handle(EngineCommand::ReorderRoomTags {
            ids: vec![beta.id.clone(), alpha.id.clone()],
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_argument", "message: {}", err.message);

    // With the current ids it succeeds, and the order is kept.
    unit(
        &first,
        EngineCommand::ReorderRoomTags {
            ids: vec![gamma.id.clone(), beta.id.clone(), alpha.id.clone()],
        },
    )
    .await;
    assert_eq!(
        tag_ids(catalogue(&second).await),
        vec![gamma.id.clone(), beta.id.clone(), alpha.id.clone()]
    );

    // Reusing a name gets a distinct id instead of overwriting the first tag.
    let again = create_tag(&first, &format!("Alpha {run}"), TagColour::Violet).await;
    assert_ne!(again.id, alpha.id);

    // Unknown id and a rename with nothing to change are both refused.
    not_found(
        &first,
        EngineCommand::RenameRoomTag {
            id: format!("missing-{run}"),
            name: Some("x".into()),
            colour: None,
        },
    )
    .await;
    invalid(
        &first,
        EngineCommand::RenameRoomTag {
            id: alpha.id.clone(),
            name: None,
            colour: None,
        },
    )
    .await;
}
