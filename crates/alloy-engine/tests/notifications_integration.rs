//! Notification settings (defaults, mention rules, room levels) against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineResponse, NotificationLevel, NotificationSettingsDto, RoomId,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

fn engine(dir: &tempfile::TempDir) -> AppEngine {
    AppEngine::new(EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    ))
}

async fn login(eng: &AppEngine, hs: &str, username: &str) {
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.to_owned(),
        username: username.into(),
        password: "testpassword123!".into(),
    })
    .await
    .unwrap();
    // Room state reaches the store through sync, which the room list starts.
    eng.handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .unwrap();
}

async fn send(eng: &AppEngine, command: EngineCommand) -> Result<EngineResponse, String> {
    eng.handle(command).await.map_err(|e| format!("{e:?}"))
}

async fn read_settings(eng: &AppEngine) -> Result<NotificationSettingsDto, String> {
    match send(eng, EngineCommand::GetNotificationSettings).await? {
        EngineResponse::NotificationSettings { settings } => Ok(settings),
        other => Err(format!("unexpected {other:?}")),
    }
}

async fn read_room_level(
    eng: &AppEngine,
    room_id: &RoomId,
) -> Result<(NotificationLevel, bool), String> {
    match send(
        eng,
        EngineCommand::GetRoomNotificationLevel {
            room_id: room_id.clone(),
        },
    )
    .await?
    {
        EngineResponse::RoomNotificationLevel {
            level, is_default, ..
        } => Ok((level, is_default)),
        other => Err(format!("unexpected {other:?}")),
    }
}

fn ensure(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_owned()) }
}

/// Polls `GetRoomNotificationLevel` until the room is known to the client.
async fn await_room(eng: &AppEngine, room_id: &RoomId) {
    tokio::time::timeout(WAIT, async {
        loop {
            if read_room_level(eng, room_id).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("room never became known to the client");
}

async fn exercise(
    eng: &AppEngine,
    room_id: &RoomId,
    original: &NotificationSettingsDto,
) -> Result<(), String> {
    // Room mute: the room starts on the default, muting stores a room rule,
    // and removing the rule returns to the group default (room has one member).
    let (level, is_default) = read_room_level(eng, room_id).await?;
    ensure(is_default, "new room should start on the default level")?;
    ensure(
        level == original.group,
        "new room should report the group default",
    )?;

    send(
        eng,
        EngineCommand::SetRoomNotificationLevel {
            room_id: room_id.clone(),
            level: Some(NotificationLevel::Mute),
        },
    )
    .await?;
    let (level, is_default) = read_room_level(eng, room_id).await?;
    ensure(
        level == NotificationLevel::Mute,
        "muted room should report Mute",
    )?;
    ensure(!is_default, "muted room should not report a default level")?;

    send(
        eng,
        EngineCommand::SetRoomNotificationLevel {
            room_id: room_id.clone(),
            level: None,
        },
    )
    .await?;
    let (level, is_default) = read_room_level(eng, room_id).await?;
    ensure(is_default, "cleared room should report a default level")?;
    ensure(
        level == original.group,
        "cleared room should return to the group default",
    )?;

    // Group default: set Mentions and read it back; restored by the caller.
    send(
        eng,
        EngineCommand::SetDefaultNotificationLevel {
            one_to_one: false,
            level: NotificationLevel::Mentions,
        },
    )
    .await?;
    let settings = read_settings(eng).await?;
    ensure(
        settings.group == NotificationLevel::Mentions,
        "group default should be Mentions",
    )?;
    ensure(
        settings.one_to_one == original.one_to_one,
        "one-to-one default should be unchanged",
    )?;

    // Room mention rule: toggle, read back; restored by the caller.
    send(
        eng,
        EngineCommand::SetMentionRules {
            room_mention: Some(!original.room_mention),
            user_mention: None,
            invite: None,
        },
    )
    .await?;
    let settings = read_settings(eng).await?;
    ensure(
        settings.room_mention == !original.room_mention,
        "room_mention should reflect the toggle",
    )?;
    ensure(
        settings.user_mention == original.user_mention,
        "user_mention should be unchanged",
    )?;
    ensure(
        settings.invite == original.invite,
        "invite should be unchanged",
    )?;
    Ok(())
}

/// Runs every restore step even when an earlier one fails, returning the first error.
async fn restore(
    eng: &AppEngine,
    room_id: &RoomId,
    original: &NotificationSettingsDto,
) -> Result<(), String> {
    // The room is a throwaway one and the checks already cleared its rule, so this is hygiene only. The SDK
    // removes the rule from its local copy of the rules after the server call, and a sync that lands in
    // between makes that local removal fail with "Unable to remove push rule" although the server call worked
    // (seen on slower CI machines, never locally). The account-wide settings below stay strict.
    if let Err(error) = send(
        eng,
        EngineCommand::SetRoomNotificationLevel {
            room_id: room_id.clone(),
            level: None,
        },
    )
    .await
    {
        eprintln!("clearing the test room's rule failed (ignored): {error}");
    }
    let commands = [
        EngineCommand::SetDefaultNotificationLevel {
            one_to_one: true,
            level: original.one_to_one,
        },
        EngineCommand::SetDefaultNotificationLevel {
            one_to_one: false,
            level: original.group,
        },
        EngineCommand::SetMentionRules {
            room_mention: Some(original.room_mention),
            user_mention: Some(original.user_mention),
            invite: Some(original.invite),
        },
    ];
    let mut result = Ok(());
    for command in commands {
        let step = send(eng, command).await.map(drop);
        result = result.and(step);
    }
    result
}

#[tokio::test]
async fn notification_settings_against_synapse() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };
    let alice_dir = tempfile::tempdir().unwrap();
    let alice = engine(&alice_dir);
    login(&alice, &hs, "alice").await;

    let original = read_settings(&alice)
        .await
        .expect("reading notification settings");

    let room_id = match alice
        .handle(EngineCommand::CreateRoom {
            name: "notifications-test".into(),
            encrypted: false,
            invite: None,
        })
        .await
        .unwrap()
    {
        EngineResponse::RoomCreated { room_id } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    await_room(&alice, &room_id).await;

    let outcome = exercise(&alice, &room_id, &original).await;
    let restored = restore(&alice, &room_id, &original).await;
    outcome.expect("notification checks failed");
    restored.expect("restoring notification settings failed");
}
