//! End-to-end engine behaviour against the deterministic fake connector.
//!
//! Every test drives the real `AppEngine`: login state machine, store persistence, projections,
//! outbox pipeline and capability gates. Nothing is mocked below the connector seam.

use std::sync::Arc;
use std::time::Duration;

use alloy_connector::fake::{FAKE_CHAT_DIRECT, FAKE_CODE, FAKE_USER_ID};
use alloy_engine::factory::FakeConnectorFactory;
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    Account, AccountState, EngineCommand, EngineEvent, EngineResponse, EventSendState, LoginId,
    LoginInputDto, LoginStepDto, Network, RoomId, RoomSummary, SubscriptionId, TimelineItemContent,
    TimelineItemDto, VectorDiffDto,
};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::{SendState, Store};
use tempfile::TempDir;
use tokio::sync::broadcast;

const KEY: [u8; 32] = [7; 32];
const WAIT: Duration = Duration::from_secs(10);

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
    assert_eq!(values.len(), 3);
    let titles: Vec<&str> = values
        .iter()
        .map(|summary| summary.name.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(titles, ["Ada Lovelace", "Analytical Engine", "Alloy News"]);
    // The seeded channel is archived on the network; the list still carries it.
    assert!(values[2].archived);
    assert!(values[0].pinned);
    assert_eq!(values[0].unread_notifications.notification_count, 2);
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
    assert_eq!(accounts[0].id, account.id, "the same account id is restored");

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
