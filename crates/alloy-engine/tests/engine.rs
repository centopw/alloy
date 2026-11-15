//! End-to-end engine behaviour against the deterministic fake connector.
//!
//! Every test drives the real `AppEngine`: login state machine, store persistence, projections,
//! outbox pipeline and capability gates. Nothing is mocked below the connector seam.

use std::sync::Arc;
use std::time::Duration;

use alloy_connector::fake::{
    FAKE_CHAT_CHANNEL, FAKE_CHAT_DIRECT, FAKE_CHAT_FORUM, FAKE_CHAT_GROUP, FAKE_CODE, FAKE_USER_ID,
    FakeConnector,
};
use alloy_connector::{ConnectionState, Connector, ConnectorEvent};
use alloy_engine::factory::FakeConnectorFactory;
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    Account, AccountState, ConnectionStatus, EngineCommand, EngineEvent, EngineResponse, EventId,
    EventSendState, LoginId, LoginInputDto, LoginStepDto, Network, RoomId, RoomSummary,
    SubscriptionId, TimelineItemContent, TimelineItemDto, UserId, VectorDiffDto,
};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::{SendState, Store};
use tempfile::TempDir;
use tokio::sync::broadcast;

const KEY: [u8; 32] = [7; 32];
const WAIT: Duration = Duration::from_secs(10);
const FAKE_TOPIC_GENERAL: &str = "fake-chat-forum/t1";
const FAKE_TOPIC_OTHER: &str = "fake-chat-forum/t2";

struct Harness {
    engine: AppEngine,
    _dir: TempDir,
    factory: Arc<FakeConnectorFactory>,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let factory = Arc::new(FakeConnectorFactory::new());
        let store = Store::open_in_memory(KEY).expect("open store");
        let ctx =
            EngineContext::with_store(store, dir.path().to_path_buf(), secrets, factory.clone());
        Self {
            engine: AppEngine::new(ctx),
            _dir: dir,
            factory,
        }
    }

    fn events(&self) -> broadcast::Receiver<EngineEvent> {
        self.engine.sink().subscribe()
    }
}

async fn wait_for<F>(events: &mut broadcast::Receiver<EngineEvent>, mut matches: F) -> EngineEvent
where
    F: FnMut(&EngineEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Ok(event)) => {
                if matches(&event) {
                    return event;
                }
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => panic!("the event stream closed"),
            Err(_) => panic!("timed out waiting for an engine event"),
        }
    }
}

async fn begin_login(engine: &AppEngine) -> (LoginId, LoginStepDto) {
    match engine
        .handle(EngineCommand::BeginLogin {
            network: Network::Fake,
        })
        .await
        .expect("begin login")
    {
        EngineResponse::LoginStep { login_id, step } => (login_id, step),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn submit_login(
    engine: &AppEngine,
    login_id: &LoginId,
    input: LoginInputDto,
) -> LoginStepDto {
    match engine
        .handle(EngineCommand::SubmitLogin {
            login_id: login_id.clone(),
            input,
        })
        .await
        .expect("submit login")
    {
        EngineResponse::LoginStep { step, .. } => step,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn login(engine: &AppEngine) -> Account {
    let (login_id, step) = begin_login(engine).await;
    assert_eq!(step, LoginStepDto::PhoneNumber);
    let step = submit_login(
        engine,
        &login_id,
        LoginInputDto::PhoneNumber {
            phone: "+15550100".to_string(),
        },
    )
    .await;
    assert!(matches!(step, LoginStepDto::Code { .. }));
    let step = submit_login(
        engine,
        &login_id,
        LoginInputDto::Code {
            code: FAKE_CODE.to_string(),
        },
    )
    .await;
    match step {
        LoginStepDto::Done { account } => account,
        other => panic!("login did not finish: {other:?}"),
    }
}

async fn list_accounts(engine: &AppEngine) -> Vec<Account> {
    match engine
        .handle(EngineCommand::ListAccounts)
        .await
        .expect("list accounts")
    {
        EngineResponse::AccountsList { accounts } => accounts,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn chat_id_by_title(engine: &AppEngine, title: &str) -> RoomId {
    let store = &engine.context().store;
    let mut chats = store.list_chats(None, false, 50).await.expect("list chats");
    chats.extend(
        store
            .list_chats(None, true, 50)
            .await
            .expect("list archived"),
    );
    chats
        .into_iter()
        .find(|chat| chat.title.as_deref() == Some(title))
        .map(|chat| RoomId(chat.chat_id))
        .unwrap_or_else(|| panic!("chat '{title}' was not stored"))
}

async fn subscribe_room_list(engine: &AppEngine) -> SubscriptionId {
    match engine
        .handle(EngineCommand::SubscribeRoomList { filter: None })
        .await
        .expect("subscribe room list")
    {
        EngineResponse::RoomListSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn subscribe_timeline(engine: &AppEngine, room_id: &RoomId) -> SubscriptionId {
    match engine
        .handle(EngineCommand::SubscribeTimeline {
            room_id: room_id.clone(),
        })
        .await
        .expect("subscribe timeline")
    {
        EngineResponse::TimelineSubscribed { subscription_id } => subscription_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn timeline_values(diff: &VectorDiffDto<TimelineItemDto>) -> Vec<TimelineItemDto> {
    match diff {
        VectorDiffDto::Reset { values } | VectorDiffDto::Append { values } => values.clone(),
        VectorDiffDto::PushFront { value } | VectorDiffDto::PushBack { value } => {
            vec![value.clone()]
        }
        VectorDiffDto::Insert { value, .. } | VectorDiffDto::Set { value, .. } => {
            vec![value.clone()]
        }
        _ => Vec::new(),
    }
}

fn room_values(diff: &VectorDiffDto<RoomSummary>) -> Vec<RoomSummary> {
    match diff {
        VectorDiffDto::Reset { values } | VectorDiffDto::Append { values } => values.clone(),
        VectorDiffDto::PushFront { value } | VectorDiffDto::PushBack { value } => {
            vec![value.clone()]
        }
        VectorDiffDto::Insert { value, .. } | VectorDiffDto::Set { value, .. } => {
            vec![value.clone()]
        }
        _ => Vec::new(),
    }
}

fn text_body(item: &TimelineItemDto) -> Option<&str> {
    match &item.content {
        TimelineItemContent::TextMessage { body, .. } => Some(body.as_str()),
        _ => None,
    }
}

fn has_sent_message(diff: &VectorDiffDto<TimelineItemDto>, body: &str) -> bool {
    timeline_values(diff).iter().any(|item| {
        text_body(item) == Some(body)
            && matches!(&item.send_state, EventSendState::Sent { event_id: Some(_) })
    })
}

fn has_incoming_message(diff: &VectorDiffDto<TimelineItemDto>, body: &str) -> bool {
    timeline_values(diff)
        .iter()
        .any(|item| text_body(item) == Some(body) && !item.is_own)
}

fn room_has_unread(diff: &VectorDiffDto<RoomSummary>, room_id: &RoomId, unread: u64) -> bool {
    room_values(diff).iter().any(|summary| {
        summary.id == *room_id && summary.unread_notifications.notification_count == unread
    })
}

#[tokio::test]
async fn login_state_machine_creates_an_account() {
    let harness = Harness::new();
    let mut events = harness.events();

    // A wrong code fails the attempt and stores nothing.
    let (login_id, step) = begin_login(&harness.engine).await;
    assert_eq!(step, LoginStepDto::PhoneNumber);
    let step = submit_login(
        &harness.engine,
        &login_id,
        LoginInputDto::PhoneNumber {
            phone: "+15550100".to_string(),
        },
    )
    .await;
    assert!(matches!(step, LoginStepDto::Code { .. }));
    let step = submit_login(
        &harness.engine,
        &login_id,
        LoginInputDto::Code {
            code: "00000".to_string(),
        },
    )
    .await;
    match step {
        LoginStepDto::Failed { reason } => match reason {
            alloy_protocol::LoginFailureDto::InvalidInput { field, .. } => {
                assert_eq!(field.as_deref(), Some("code"));
            }
            other => panic!("unexpected failure: {other:?}"),
        },
        other => panic!("wrong code must fail: {other:?}"),
    }
    assert!(list_accounts(&harness.engine).await.is_empty());

    // A fresh attempt with the right code finishes and announces the account.
    let account = login(&harness.engine).await;
    assert_eq!(account.network, Network::Fake);
    assert_eq!(account.state, AccountState::Active);
    assert_eq!(
        account.remote_user_id.as_ref().map(|id| id.0.as_str()),
        Some(FAKE_USER_ID)
    );
    assert!(account.capabilities.reactions);

    let accounts = list_accounts(&harness.engine).await;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, account.id);

    let changed = wait_for(
        &mut events,
        |event| matches!(event, EngineEvent::AccountsChanged { accounts } if accounts.len() == 1),
    )
    .await;
    match changed {
        EngineEvent::AccountsChanged { accounts } => assert_eq!(accounts[0].id, account.id),
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn room_list_reset_contains_the_seeded_chats() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let mut events = harness.events();
    let _subscription = subscribe_room_list(&harness.engine).await;

    let event = wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::RoomListDiff { diff, .. }
                if matches!(diff.as_ref(), VectorDiffDto::Reset { .. })
        )
    })
    .await;

    let EngineEvent::RoomListDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let VectorDiffDto::Reset { values } = *diff else {
        panic!("expected a reset")
    };
    // Three seeded chats, plus the forum and its two topics.
    assert_eq!(values.len(), 6);
    let row = |title: &str| {
        values
            .iter()
            .find(|summary| summary.name.as_deref() == Some(title))
            .unwrap_or_else(|| panic!("the row '{title}'"))
    };
    row("Analytical Engine");
    // The seeded channel is archived on the network; the list still carries it.
    assert!(row("Alloy News").archived);
    assert!(row("Ada Lovelace").pinned);
    assert_eq!(
        row("Ada Lovelace").unread_notifications.notification_count,
        2
    );
}

#[tokio::test]
async fn timeline_reset_replays_seeded_history() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Ada Lovelace").await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;

    let event = wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::TimelineDiff { diff, .. }
                if matches!(diff.as_ref(), VectorDiffDto::Reset { .. })
        )
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let VectorDiffDto::Reset { values } = *diff else {
        panic!("expected a reset")
    };

    let bodies: Vec<&str> = values.iter().filter_map(text_body).collect();
    assert_eq!(
        bodies,
        [
            "Are you free for a walk?",
            "The engine sketch is ready.",
            "I pushed the notes.",
            "Thanks!",
            "Talk tomorrow."
        ]
    );
    assert!(
        values
            .iter()
            .any(|item| matches!(&item.content, TimelineItemContent::DayDivider { .. })),
        "the timeline starts a day section"
    );
}

#[tokio::test]
async fn send_text_reaches_sent_once_and_reconciles_the_echo() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Ada Lovelace").await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;

    let response = harness
        .engine
        .handle(EngineCommand::SendText {
            room_id: room.clone(),
            body: "hello from the test".to_string(),
            reply_to: None,
            txn_id: None,
        })
        .await
        .expect("send text");
    let EngineResponse::MessageQueued { txn_id } = response else {
        panic!("unexpected response: {response:?}")
    };

    let echo = harness
        .engine
        .context()
        .store
        .find_message_by_txn(&txn_id)
        .await
        .expect("find echo")
        .expect("the local echo exists");
    assert!(matches!(
        echo.send_state,
        Some(SendState::Queued | SendState::Sending | SendState::Sent)
    ));
    assert!(echo.is_own);

    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => has_sent_message(diff, "hello from the test"),
        _ => false,
    })
    .await;

    let records = harness
        .engine
        .context()
        .store
        .page_messages(&room.0, None, 50)
        .await
        .expect("page messages");
    let matching: Vec<_> = records
        .iter()
        .filter(|record| record.body.as_deref() == Some("hello from the test"))
        .collect();
    assert_eq!(matching.len(), 1, "the echo must reconcile, not duplicate");
    assert_eq!(matching[0].send_state, Some(SendState::Sent));
    assert!(
        matching[0]
            .remote_msg_id
            .as_deref()
            .is_some_and(|id| id.starts_with("fake-msg-sent-")),
        "the network id is stored"
    );
}

#[tokio::test]
async fn incoming_message_updates_timeline_and_unread() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Ada Lovelace").await;
    // One receiver per projection: the room list and timeline updates arrive in either order, and
    // a shared receiver would let the first wait consume the other projection's event.
    let mut timeline_events = harness.events();
    let mut room_events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;
    let _rooms = subscribe_room_list(&harness.engine).await;
    // Drain the initial reset so the next match is a live update.
    wait_for(&mut room_events, |event| {
        matches!(event, EngineEvent::RoomListDiff { diff, .. } if matches!(diff.as_ref(), VectorDiffDto::Reset { .. }))
    })
    .await;

    let fake = harness.factory.last().expect("the running connector");
    fake.inject_incoming(FAKE_CHAT_DIRECT, "fake-user-ada", "ping from the network")
        .await
        .expect("inject a message");

    wait_for(&mut timeline_events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => {
            has_incoming_message(diff, "ping from the network")
        }
        _ => false,
    })
    .await;

    wait_for(&mut room_events, |event| match event {
        EngineEvent::RoomListDiff { diff, .. } => room_has_unread(diff, &room, 3),
        _ => false,
    })
    .await;
}

#[tokio::test]
async fn mark_room_read_clears_the_unread_count() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Ada Lovelace").await;
    let mut events = harness.events();
    let _rooms = subscribe_room_list(&harness.engine).await;

    let response = harness
        .engine
        .handle(EngineCommand::MarkRoomRead {
            room_id: room.clone(),
        })
        .await
        .expect("mark read");
    assert_eq!(response, EngineResponse::RoomFlagsUpdated);

    wait_for(&mut events, |event| match event {
        EngineEvent::RoomListDiff { diff, .. } => room_has_unread(diff, &room, 0),
        _ => false,
    })
    .await;
}

#[tokio::test]
async fn restart_restores_accounts_and_messages() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let factory = Arc::new(FakeConnectorFactory::new());
    let database = dir.path().join("alloy.db");

    let store = Store::open(&database, KEY).expect("open store");
    let ctx = EngineContext::with_store(
        store,
        dir.path().to_path_buf(),
        secrets.clone(),
        factory.clone(),
    );
    let engine = AppEngine::new(ctx);
    let account = login(&engine).await;
    let room = chat_id_by_title(&engine, "Ada Lovelace").await;

    // The timeline has to be open before the send: only a subscription turns a store change into
    // a `TimelineDiff`, so waiting for one without subscribing could never succeed.
    let mut events = engine.sink().subscribe();
    let _subscription = subscribe_timeline(&engine, &room).await;
    engine
        .handle(EngineCommand::SendText {
            room_id: room,
            body: "persisted message".to_string(),
            reply_to: None,
            txn_id: None,
        })
        .await
        .expect("send text");
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => has_sent_message(diff, "persisted message"),
        _ => false,
    })
    .await;

    engine.stop().await;
    // Aborted tasks may still be finishing a blocking store write; let them settle before a
    // second connection opens the same file.
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(engine);

    let store = Store::open(&database, KEY).expect("reopen store");
    let ctx = EngineContext::with_store(store, dir.path().to_path_buf(), secrets, factory);
    let engine = AppEngine::new(ctx);
    engine.start().await.expect("start engine");

    let accounts = list_accounts(&engine).await;
    assert_eq!(accounts.len(), 1, "the account survives the restart");
    assert_eq!(
        accounts[0].id, account.id,
        "the same account id is restored"
    );

    let room = chat_id_by_title(&engine, "Ada Lovelace").await;
    let records = engine
        .context()
        .store
        .page_messages(&room.0, None, 50)
        .await
        .expect("page the persisted messages");
    let persisted: Vec<_> = records
        .iter()
        .filter(|record| record.body.as_deref() == Some("persisted message"))
        .collect();
    assert_eq!(persisted.len(), 1, "the message survives the restart once");
    assert_eq!(persisted[0].send_state, Some(SendState::Sent));
    assert!(
        persisted[0]
            .remote_msg_id
            .as_deref()
            .is_some_and(|id| id.starts_with("fake-msg-sent-")),
        "the network id is persisted"
    );
}

#[tokio::test]
async fn logout_removes_every_local_trace() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let mut events = harness.events();

    let response = harness
        .engine
        .handle(EngineCommand::Logout {
            account_id: account.id.clone(),
        })
        .await
        .expect("logout");
    assert_eq!(
        response,
        EngineResponse::LoggedOut {
            account_id: account.id.clone()
        }
    );

    assert!(list_accounts(&harness.engine).await.is_empty());
    let store = &harness.engine.context().store;
    assert!(
        store
            .list_chats(None, false, 50)
            .await
            .expect("list chats")
            .is_empty()
    );
    assert!(
        store
            .get_account(&account.id.0)
            .await
            .expect("get account")
            .is_none()
    );

    let _rooms = subscribe_room_list(&harness.engine).await;
    let event = wait_for(&mut events, |event| {
        matches!(event, EngineEvent::RoomListDiff { diff, .. } if matches!(diff.as_ref(), VectorDiffDto::Reset { .. }))
    })
    .await;
    let EngineEvent::RoomListDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let VectorDiffDto::Reset { values } = *diff else {
        panic!("expected a reset")
    };
    assert!(values.is_empty());
}

#[tokio::test]
async fn capability_gate_rejects_unsupported_commands() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let factory = Arc::new(FakeConnectorFactory::with_capability_override(
        |capabilities| {
            capabilities.typing = false;
        },
    ));
    let store = Store::open_in_memory(KEY).expect("open store");
    let ctx = EngineContext::with_store(store, dir.path().to_path_buf(), secrets, factory);
    let engine = AppEngine::new(ctx);
    login(&engine).await;
    let room = chat_id_by_title(&engine, "Ada Lovelace").await;

    let error = engine
        .handle(EngineCommand::SetTyping {
            room_id: room,
            typing: true,
        })
        .await
        .expect_err("typing is not supported on this connector");
    assert_eq!(error.code, "not_supported");
    assert_eq!(error.reason.as_deref(), Some("typing"));
}

#[tokio::test]
async fn get_media_caches_the_whole_file_under_its_final_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let factory = Arc::new(FakeConnectorFactory::new());
    let store = Store::open_in_memory(KEY).expect("open store");
    let ctx = EngineContext::with_store(store.clone(), dir.path().to_path_buf(), secrets, factory);
    let engine = AppEngine::new(ctx);
    let account = login(&engine).await;
    let media_id = store
        .insert_media(alloy_store::NewMedia {
            account_id: account.id.0.clone(),
            remote_ref: "clip-1".to_string(),
            mime: Some("video/mp4".to_string()),
            size_bytes: None,
            width: None,
            height: None,
            duration_ms: None,
            file_name: None,
        })
        .await
        .expect("insert media");

    let get = || EngineCommand::GetMedia {
        source: alloy_protocol::MediaSource {
            media_id: media_id.clone(),
        },
        kind: "thumbnail".to_string(),
        // The caller's type never renames what the network says the file is.
        mime_type: Some("image/jpeg".to_string()),
    };
    // One media object is requested several times at once (an avatar at two sizes, a tile and
    // its full view); every request must see the whole file.
    let (first, second) = tokio::join!(engine.handle(get()), engine.handle(get()));
    for response in [first, second] {
        let EngineResponse::Media { path } = response.expect("get media") else {
            panic!("unexpected response");
        };
        assert_eq!(
            std::path::Path::new(&path),
            dir.path().join("media").join(format!("{media_id}.mp4"))
        );
        assert_eq!(
            std::fs::read(&path).expect("cached file"),
            b"fake-media:clip-1:video/mp4"
        );
    }
    // Only the finished file stays in the cache: no partial download is left behind.
    let names: Vec<_> = std::fs::read_dir(dir.path().join("media"))
        .expect("media dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from(format!("{media_id}.mp4"))]);
}

#[tokio::test]
async fn room_members_come_from_the_network() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Analytical Engine").await;

    let response = harness
        .engine
        .handle(EngineCommand::GetRoomMembers {
            room_id: room.clone(),
        })
        .await
        .expect("list members");
    let EngineResponse::RoomMembers { members } = response else {
        panic!("unexpected response: {response:?}")
    };
    let listed: Vec<(&str, Option<&str>, Option<&str>)> = members
        .iter()
        .map(|member| {
            (
                member.user_id.0.as_str(),
                member.display_name.as_deref(),
                member.role.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("fake-user-charles", Some("Charles Babbage"), Some("owner")),
            (FAKE_USER_ID, Some("Fake User"), None),
        ]
    );

    // The answer is stored, so the timeline can name the senders it already listed.
    let stored = harness
        .engine
        .context()
        .store
        .list_participants(&room.0)
        .await
        .expect("stored members");
    assert_eq!(stored.len(), 2);
}

#[tokio::test]
async fn a_group_sender_is_named_without_the_member_list() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Analytical Engine").await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;

    // Ada is not among the members the network lists for this group, and the member list was
    // never asked for: the profile delivered with her message is all there is.
    let fake = harness.factory.last().expect("the running connector");
    fake.inject_incoming(FAKE_CHAT_GROUP, "fake-user-ada", "late reply")
        .await
        .expect("inject a message");

    let event = wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => has_incoming_message(diff, "late reply"),
        _ => false,
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let message = timeline_values(&diff)
        .into_iter()
        .find(|item| text_body(item) == Some("late reply"))
        .expect("the message");
    assert_eq!(message.sender_display_name.as_deref(), Some("Ada Lovelace"));

    // Naming a sender does not make her a member.
    let response = harness
        .engine
        .handle(EngineCommand::GetRoomMembers { room_id: room })
        .await
        .expect("list members");
    let EngineResponse::RoomMembers { members } = response else {
        panic!("unexpected response: {response:?}")
    };
    assert!(
        members
            .iter()
            .all(|member| member.user_id.0 != "fake-user-ada")
    );
}

#[tokio::test]
async fn channel_post_is_authored_by_the_channel() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Alloy News").await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;

    let fake = harness.factory.last().expect("the running connector");
    // A broadcast post carries no user: the connector names the channel as its sender.
    fake.inject_incoming(FAKE_CHAT_CHANNEL, FAKE_CHAT_CHANNEL, "release notes")
        .await
        .expect("inject a post");

    let event = wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => has_incoming_message(diff, "release notes"),
        _ => false,
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let post = timeline_values(&diff)
        .into_iter()
        .find(|item| text_body(item) == Some("release notes"))
        .expect("the post");
    assert_eq!(post.sender_display_name.as_deref(), Some("Alloy News"));
}

/// The top-level room list by title: the forum and its topics are left out, because the pin tests
/// are about the three seeded chats.
async fn room_titles(engine: &AppEngine) -> Vec<String> {
    alloy_engine::projection::query_room_list(engine.context(), None)
        .await
        .expect("room list")
        .into_iter()
        .filter(|summary| !summary.is_forum && summary.parent_id.is_none())
        .map(|summary| summary.name.unwrap_or_default())
        .collect()
}

#[tokio::test]
async fn pinning_a_chat_reaches_the_network_and_tops_the_list() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let group = chat_id_by_title(&harness.engine, "Analytical Engine").await;
    let direct = chat_id_by_title(&harness.engine, "Ada Lovelace").await;

    // The direct chat is seeded pinned; the group is pinned by the user and goes to the top.
    let response = harness
        .engine
        .handle(EngineCommand::SetRoomPinned {
            room_id: group.clone(),
            pinned: true,
        })
        .await
        .expect("pin");
    assert_eq!(response, EngineResponse::RoomFlagsUpdated);
    assert_eq!(
        room_titles(&harness.engine).await,
        ["Analytical Engine", "Ada Lovelace", "Alloy News"]
    );
    let summaries = alloy_engine::projection::query_room_list(harness.engine.context(), None)
        .await
        .expect("room list");
    assert_eq!(summaries[0].pin_order, Some(0));
    assert_eq!(summaries[1].pin_order, Some(1));
    assert_eq!(summaries[2].pin_order, None);

    // The network was told: the fake's own list now has the group first.
    let fake = harness.factory.last().expect("the running connector");
    let page = fake.list_chats(None, 10).await.expect("chats");
    let group_remote = page
        .items
        .iter()
        .find(|chat| chat.title == "Analytical Engine")
        .expect("the group");
    assert!(group_remote.is_pinned);
    assert_eq!(group_remote.pin_order, Some(0));

    // The user drags the direct chat back above the group.
    let response = harness
        .engine
        .handle(EngineCommand::ReorderPinnedRooms {
            account_id: account.id.clone(),
            room_ids: vec![direct.clone(), group.clone()],
        })
        .await
        .expect("reorder");
    assert_eq!(
        response,
        EngineResponse::PinnedRoomsReordered {
            room_ids: vec![direct.clone(), group.clone()],
        }
    );
    assert_eq!(
        room_titles(&harness.engine).await,
        ["Ada Lovelace", "Analytical Engine", "Alloy News"]
    );
    let page = fake.list_chats(None, 10).await.expect("chats");
    let orders: Vec<(String, Option<u32>)> = page
        .items
        .iter()
        .filter(|chat| chat.is_pinned)
        .map(|chat| (chat.title.clone(), chat.pin_order))
        .collect();
    assert!(orders.contains(&("Ada Lovelace".to_string(), Some(0))));
    assert!(orders.contains(&("Analytical Engine".to_string(), Some(1))));

    // A stale list (one pinned chat missing) is refused and changes nothing.
    let refused = harness
        .engine
        .handle(EngineCommand::ReorderPinnedRooms {
            account_id: account.id.clone(),
            room_ids: vec![group.clone()],
        })
        .await
        .expect_err("a stale list");
    assert_eq!(refused.code, "invalid_argument");
    assert_eq!(
        room_titles(&harness.engine).await,
        ["Ada Lovelace", "Analytical Engine", "Alloy News"]
    );

    // Unpinning closes the gap and the group falls back into the activity order.
    harness
        .engine
        .handle(EngineCommand::SetRoomPinned {
            room_id: direct,
            pinned: false,
        })
        .await
        .expect("unpin");
    let summaries = alloy_engine::projection::query_room_list(harness.engine.context(), None)
        .await
        .expect("room list");
    assert_eq!(summaries[0].name.as_deref(), Some("Analytical Engine"));
    assert_eq!(summaries[0].pin_order, Some(0));
    assert!(!summaries[1].pinned);
}

#[tokio::test]
async fn pins_made_in_the_official_app_follow_the_events() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let mut events = harness.events();
    let _rooms = subscribe_room_list(&harness.engine).await;
    wait_for(&mut events, |event| {
        matches!(event, EngineEvent::RoomListDiff { diff, .. } if matches!(diff.as_ref(), VectorDiffDto::Reset { .. }))
    })
    .await;

    // The network reorders: the group first, then the direct chat. Archived chats are outside the
    // pinned block, so the archived channel is not used here.
    let fake = harness.factory.last().expect("the running connector");
    fake.reorder_pinned_chats(&[FAKE_CHAT_GROUP.to_string(), FAKE_CHAT_DIRECT.to_string()])
        .await
        .expect("reorder on the network");
    wait_for(&mut events, |event| match event {
        EngineEvent::RoomListDiff { diff, .. } => room_values(diff)
            .first()
            .is_some_and(|room| room.name.as_deref() == Some("Analytical Engine") && room.pinned),
        _ => false,
    })
    .await;
    assert_eq!(
        room_titles(&harness.engine).await,
        ["Analytical Engine", "Ada Lovelace", "Alloy News"]
    );

    // Unpinned on the network: the chat drops into the activity order, below the pinned direct chat.
    fake.set_chat_pinned(FAKE_CHAT_GROUP, false)
        .await
        .expect("unpin on the network");
    wait_for(&mut events, |event| match event {
        EngineEvent::RoomListDiff { diff, .. } => room_values(diff)
            .iter()
            .any(|room| room.name.as_deref() == Some("Analytical Engine") && !room.pinned),
        _ => false,
    })
    .await;
    assert_eq!(
        room_titles(&harness.engine).await,
        ["Ada Lovelace", "Analytical Engine", "Alloy News"]
    );
}

#[tokio::test]
async fn pinned_messages_round_trip_and_show_in_the_bar() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let room = chat_id_by_title(&harness.engine, "Analytical Engine").await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &room).await;
    let event = wait_for(&mut events, |event| {
        matches!(event, EngineEvent::TimelineDiff { diff, .. } if matches!(diff.as_ref(), VectorDiffDto::Reset { .. }))
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let items = timeline_values(&diff);
    let target = items
        .iter()
        .find(|item| text_body(item) == Some("Works for me."))
        .expect("a seeded message");
    assert!(!target.pinned);
    let event_id = target.event_id.clone().expect("a stored message");

    let response = harness
        .engine
        .handle(EngineCommand::SetMessagePinned {
            room_id: room.clone(),
            event_id: event_id.clone(),
            pinned: true,
        })
        .await
        .expect("pin the message");
    assert_eq!(response, EngineResponse::MessagePinSet { pinned: true });

    // The timeline carries the flag and the bar lists the message.
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => timeline_values(diff)
            .iter()
            .any(|item| item.event_id.as_ref() == Some(&event_id) && item.pinned),
        _ => false,
    })
    .await;
    let response = harness
        .engine
        .handle(EngineCommand::GetPinnedMessages {
            room_id: room.clone(),
        })
        .await
        .expect("list pinned");
    let EngineResponse::PinnedMessages { items } = response else {
        panic!("unexpected response: {response:?}")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(text_body(&items[0]), Some("Works for me."));

    // The network was told, and its own unpin comes back through the event stream.
    let fake = harness.factory.last().expect("the running connector");
    let page = fake
        .backfill(FAKE_CHAT_GROUP, None, 10)
        .await
        .expect("history");
    let remote = page
        .items
        .iter()
        .find(|message| message.body.as_deref() == Some("Works for me."))
        .expect("the message on the network");
    assert!(remote.pinned);
    fake.set_message_pinned(FAKE_CHAT_GROUP, &remote.remote_msg_id, false)
        .await
        .expect("unpin on the network");
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => timeline_values(diff)
            .iter()
            .any(|item| item.event_id.as_ref() == Some(&event_id) && !item.pinned),
        _ => false,
    })
    .await;
    let response = harness
        .engine
        .handle(EngineCommand::GetPinnedMessages { room_id: room })
        .await
        .expect("list pinned");
    let EngineResponse::PinnedMessages { items } = response else {
        panic!("unexpected response: {response:?}")
    };
    assert!(items.is_empty());
}

/// The local id of a chat the account stores under its network id.
async fn local_chat(engine: &AppEngine, account: &Account, remote_chat_id: &str) -> RoomId {
    engine
        .context()
        .store
        .find_chat(&account.id.0, remote_chat_id)
        .await
        .expect("find chat")
        .map(|chat| RoomId(chat.chat_id))
        .unwrap_or_else(|| panic!("chat '{remote_chat_id}' was not stored"))
}

#[tokio::test]
async fn forum_topics_are_rooms_nested_under_the_forum() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let forum = local_chat(&harness.engine, &account, FAKE_CHAT_FORUM).await;

    let rooms = alloy_engine::projection::query_room_list(harness.engine.context(), None)
        .await
        .expect("room list");
    let forum_row = rooms
        .iter()
        .find(|summary| summary.id == forum)
        .expect("the forum row");
    assert!(forum_row.is_forum);
    assert_eq!(forum_row.parent_id, None);

    for remote in [FAKE_TOPIC_GENERAL, FAKE_TOPIC_OTHER] {
        let topic = local_chat(&harness.engine, &account, remote).await;
        let row = rooms
            .iter()
            .find(|summary| summary.id == topic)
            .unwrap_or_else(|| panic!("the topic row '{remote}'"));
        assert_eq!(row.parent_id, Some(forum.clone()));
        assert!(!row.is_forum);
        assert!(!row.pinned, "a topic is never pinned on its own");
    }
}

#[tokio::test]
async fn timeline_of_a_topic_replays_only_that_topic() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let topic = local_chat(&harness.engine, &account, FAKE_TOPIC_GENERAL).await;
    let mut events = harness.events();
    let _subscription = subscribe_timeline(&harness.engine, &topic).await;

    let event = wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::TimelineDiff { diff, .. }
                if matches!(diff.as_ref(), VectorDiffDto::Reset { .. })
        )
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    let VectorDiffDto::Reset { values } = *diff else {
        panic!("expected a reset")
    };

    let fake = harness.factory.last().expect("the running connector");
    let mut shown: Vec<String> = values
        .iter()
        .filter_map(text_body)
        .map(str::to_string)
        .collect();
    shown.sort();
    assert!(!shown.is_empty(), "the topic has history to replay");

    let page = fake
        .backfill(FAKE_TOPIC_GENERAL, None, 50)
        .await
        .expect("topic history");
    let mut expected: Vec<String> = page.items.iter().filter_map(|m| m.body.clone()).collect();
    expected.sort();
    assert_eq!(
        shown, expected,
        "the timeline holds exactly this topic's messages"
    );

    let page = fake
        .backfill(FAKE_TOPIC_OTHER, None, 50)
        .await
        .expect("other topic history");
    for message in page.items.iter().filter_map(|m| m.body.as_deref()) {
        assert!(
            !shown.iter().any(|body| body == message),
            "the other topic's message '{message}' must not appear here"
        );
    }
}

/// The remote id of the seeded message with this body, read from the fake's history.
async fn remote_message_id(fake: &FakeConnector, remote_chat_id: &str, body: &str) -> String {
    let page = fake
        .backfill(remote_chat_id, None, 50)
        .await
        .expect("history");
    page.items
        .iter()
        .find(|message| message.body.as_deref() == Some(body))
        .unwrap_or_else(|| panic!("the seeded message '{body}'"))
        .remote_msg_id
        .clone()
}

/// The local event id of the timeline item carrying this body.
fn event_id_with_body(items: &[TimelineItemDto], body: &str) -> EventId {
    items
        .iter()
        .find(|item| text_body(item) == Some(body))
        .unwrap_or_else(|| panic!("the timeline item '{body}'"))
        .event_id
        .clone()
        .expect("a stored message")
}

/// Subscribes to a room's timeline. Returns the subscription, which must stay alive for the test,
/// the event stream from before the subscribe, and the items of the initial reset.
async fn open_timeline(
    harness: &Harness,
    room: &RoomId,
) -> (
    SubscriptionId,
    broadcast::Receiver<EngineEvent>,
    Vec<TimelineItemDto>,
) {
    let mut events = harness.events();
    let subscription = subscribe_timeline(&harness.engine, room).await;
    let event = wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::TimelineDiff { diff, .. }
                if matches!(diff.as_ref(), VectorDiffDto::Reset { .. })
        )
    })
    .await;
    let EngineEvent::TimelineDiff { diff, .. } = event else {
        panic!("unexpected event")
    };
    (subscription, events, timeline_values(&diff))
}

#[tokio::test]
async fn forum_deletion_lands_on_the_topic_that_holds_the_message() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let general = local_chat(&harness.engine, &account, FAKE_TOPIC_GENERAL).await;
    let other = local_chat(&harness.engine, &account, FAKE_TOPIC_OTHER).await;
    let fake = harness.factory.last().expect("the running connector");
    let body = "Welcome to the forum.";
    let remote = remote_message_id(&fake, FAKE_TOPIC_GENERAL, body).await;

    let (_subscription, mut events, items) = open_timeline(&harness, &general).await;
    let target = event_id_with_body(&items, body);

    // Telegram names a forum's delete by the forum id alone.
    fake.emit_event(ConnectorEvent::MessageDeleted {
        remote_chat_id: FAKE_CHAT_FORUM.to_string(),
        remote_msg_id: remote.clone(),
    })
    .await
    .expect("emit the forum's delete");
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => timeline_values(diff).iter().any(|item| {
            item.event_id.as_ref() == Some(&target)
                && matches!(item.content, TimelineItemContent::Deleted)
        }),
        _ => false,
    })
    .await;

    let store = &harness.engine.context().store;
    let deleted = store
        .find_message_by_remote(&general.0, &remote)
        .await
        .expect("find message")
        .expect("the message is stored in General");
    assert!(deleted.deleted_at.is_some());

    let other_messages = store
        .page_messages(&other.0, None, 50)
        .await
        .expect("other topic");
    assert!(
        other_messages
            .iter()
            .all(|message| message.deleted_at.is_none()),
        "the other topic is untouched"
    );
}

#[tokio::test]
async fn forum_pin_lands_on_the_topic_that_holds_the_message() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let general = local_chat(&harness.engine, &account, FAKE_TOPIC_GENERAL).await;
    let other = local_chat(&harness.engine, &account, FAKE_TOPIC_OTHER).await;
    let forum = local_chat(&harness.engine, &account, FAKE_CHAT_FORUM).await;
    let fake = harness.factory.last().expect("the running connector");
    let body = "Keep each topic to its subject.";
    let remote = remote_message_id(&fake, FAKE_TOPIC_GENERAL, body).await;

    let (_subscription, mut events, items) = open_timeline(&harness, &general).await;
    let target = event_id_with_body(&items, body);

    fake.emit_event(ConnectorEvent::MessagePinned {
        remote_chat_id: FAKE_CHAT_FORUM.to_string(),
        remote_msg_id: remote,
        pinned: true,
    })
    .await
    .expect("emit the forum's pin");
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => timeline_values(diff)
            .iter()
            .any(|item| item.event_id.as_ref() == Some(&target) && item.pinned),
        _ => false,
    })
    .await;

    let response = harness
        .engine
        .handle(EngineCommand::GetPinnedMessages {
            room_id: general.clone(),
        })
        .await
        .expect("list pinned in General");
    let EngineResponse::PinnedMessages { items } = response else {
        panic!("unexpected response: {response:?}")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(text_body(&items[0]), Some(body));

    for room in [other, forum] {
        let response = harness
            .engine
            .handle(EngineCommand::GetPinnedMessages { room_id: room })
            .await
            .expect("list pinned elsewhere");
        let EngineResponse::PinnedMessages { items } = response else {
            panic!("unexpected response: {response:?}")
        };
        assert!(
            items.is_empty(),
            "only the topic holding the message has a pin"
        );
    }
}

#[tokio::test]
async fn forum_read_marker_lands_on_the_topic_that_holds_the_message() {
    let harness = Harness::new();
    let account = login(&harness.engine).await;
    let general = local_chat(&harness.engine, &account, FAKE_TOPIC_GENERAL).await;
    let other = local_chat(&harness.engine, &account, FAKE_TOPIC_OTHER).await;
    let fake = harness.factory.last().expect("the running connector");
    let body = "Keep each topic to its subject.";
    let remote = remote_message_id(&fake, FAKE_TOPIC_GENERAL, body).await;

    let (_subscription, mut events, items) = open_timeline(&harness, &general).await;
    let target = event_id_with_body(&items, body);
    let reader = UserId("fake-user-charles".to_string());

    fake.emit_event(ConnectorEvent::ReadMarker {
        remote_chat_id: FAKE_CHAT_FORUM.to_string(),
        remote_user_id: reader.0.clone(),
        up_to_remote_msg_id: remote,
    })
    .await
    .expect("emit the forum's read marker");
    wait_for(&mut events, |event| match event {
        EngineEvent::TimelineDiff { diff, .. } => timeline_values(diff)
            .iter()
            .any(|item| item.event_id.as_ref() == Some(&target) && item.read_by.contains(&reader)),
        _ => false,
    })
    .await;

    let markers = harness
        .engine
        .context()
        .store
        .list_read_markers(&other.0)
        .await
        .expect("other topic markers");
    assert!(markers.is_empty(), "the other topic has no read marker");
}

// === Connection state ===

/// One offline report as the Messenger sidecar sends it: the reason plus the attempt count of the
/// connector's own retry loop.
fn offline(reason: &str, attempt: u32) -> ConnectorEvent {
    ConnectorEvent::Connection {
        state: ConnectionState::Offline {
            reason: reason.to_string(),
            attempt: Some(attempt),
        },
    }
}

/// Asserts that no warning arrives for a while; used to prove a repeated offline report is silent.
async fn expect_no_toast(events: &mut broadcast::Receiver<EngineEvent>) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Ok(EngineEvent::ToastNotice { message, .. })) => {
                panic!("a repeated offline report must not warn again, got: {message}")
            }
            Ok(Ok(_)) => continue,
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => panic!("the event stream closed"),
            Err(_) => return,
        }
    }
}

#[tokio::test]
async fn one_outage_warns_once_and_reports_every_attempt() {
    let harness = Harness::new();
    login(&harness.engine).await;
    let fake = harness.factory.last().expect("the running connector");
    let mut events = harness.events();

    fake.emit_event(offline("network unreachable", 1))
        .await
        .expect("report the first failed attempt");
    wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::ConnectionStatus {
                status: ConnectionStatus::Reconnecting {
                    attempt: 1,
                    next_retry_millis: 2_000,
                },
                ..
            }
        )
    })
    .await;
    let notice = wait_for(&mut events, |event| {
        matches!(event, EngineEvent::ToastNotice { .. })
    })
    .await;
    let EngineEvent::ToastNotice { message, level } = notice else {
        panic!("the wait only returns a notice")
    };
    assert_eq!(message, "network unreachable");
    assert_eq!(level, "warning");

    // The connector retries on its own and reports each failed attempt; the second report is the
    // same outage, so it updates the banner but must not warn again.
    fake.emit_event(offline("network unreachable", 2))
        .await
        .expect("report the second failed attempt");
    wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::ConnectionStatus {
                status: ConnectionStatus::Reconnecting {
                    attempt: 2,
                    next_retry_millis: 4_000,
                },
                ..
            }
        )
    })
    .await;
    expect_no_toast(&mut events).await;

    // Coming back online arms the next outage.
    fake.emit_event(ConnectorEvent::Connection {
        state: ConnectionState::Online,
    })
    .await
    .expect("report the recovery");
    wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::ConnectionStatus {
                status: ConnectionStatus::Connected,
                ..
            }
        )
    })
    .await;

    fake.emit_event(offline("network unreachable", 1))
        .await
        .expect("report the next outage");
    wait_for(&mut events, |event| {
        matches!(
            event,
            EngineEvent::ConnectionStatus {
                status: ConnectionStatus::Reconnecting { attempt: 1, .. },
                ..
            }
        )
    })
    .await;
    wait_for(&mut events, |event| {
        matches!(event, EngineEvent::ToastNotice { .. })
    })
    .await;
}
