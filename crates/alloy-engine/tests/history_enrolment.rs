//! Enrolling this device in the account's encrypted history archive.
//!
//! The PIN is a one-shot secret: the engine hands it to the connector, stores nothing that
//! contains it and logs nothing that contains it. The expected shortfalls are typed outcomes, not
//! command failures, and none of them is a warning.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alloy_connector::fake::{FAKE_CODE, FakeConnector};
use alloy_connector::{
    Capabilities, Connector, ConnectorError, ConnectorEvent, HistoryCapabilities, LoginInput,
    LoginStep, MediaRef, Network, OutgoingMessage, Page, RemoteChat, RemoteMember, RemoteMessage,
    RemoteUser, SendAck, SessionBlob,
};
use alloy_engine::accounts::start_with_connector;
use alloy_engine::factory::ConnectorFactory;
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    AccountId, EngineCommand, EngineResponse, HistoryEnrolmentOutcome, HistoryPin, NetworkInfoDto,
};
use alloy_storage::InMemorySecretStore;
use alloy_store::{AccountRecord, AccountState, Network as StoreNetwork, NewAccount, Store};
use tempfile::TempDir;
use tokio::sync::mpsc::Sender;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Level, Metadata, Subscriber};

const KEY: [u8; 32] = [7; 32];
const PIN: &str = "481516";

/// Records the level and rendered fields of every event dispatched to it.
#[derive(Default)]
struct LogCapture {
    events: Mutex<Vec<(Level, String)>>,
}

impl LogCapture {
    fn new() -> Self {
        Self::default()
    }

    fn snapshot(&self) -> Vec<(Level, String)> {
        self.events.lock().expect("log capture lock").clone()
    }
}

#[derive(Default)]
struct RenderedFields(String);

impl Visit for RenderedFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        let _ = write!(self.0, "{}={value:?}", field.name());
    }
}

impl Subscriber for LogCapture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = RenderedFields::default();
        event.record(&mut fields);
        self.events
            .lock()
            .expect("log capture lock")
            .push((*event.metadata().level(), fields.0));
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

/// The factory seam these tests do not use: the account is started directly with a connector.
struct UnusedFactory;

#[async_trait::async_trait]
impl ConnectorFactory for UnusedFactory {
    async fn networks(&self) -> Vec<NetworkInfoDto> {
        Vec::new()
    }

    async fn create(
        &self,
        _network: alloy_protocol::Network,
        _account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError> {
        Err(ConnectorError::NotSupported {
            feature: "test factory".to_string(),
        })
    }
}

/// A connector that predates the archive method: every required call is delegated, but
/// `enrol_history` is left to the trait's default body, which answers `NotSupported`. It can also
/// refuse `backfill` with a chosen error, so paging's honest reason can be exercised.
struct OlderConnector {
    inner: Arc<FakeConnector>,
    backfill_error: Mutex<Option<ConnectorError>>,
}

impl OlderConnector {
    fn new(inner: Arc<FakeConnector>) -> Self {
        Self {
            inner,
            backfill_error: Mutex::new(None),
        }
    }

    /// Refuses later backfills, after the setup's own sync has stored its history.
    fn refuse_backfill(&self, error: ConnectorError) {
        *self.backfill_error.lock().expect("backfill lock") = Some(error);
    }
}

#[async_trait::async_trait]
impl Connector for OlderConnector {
    fn network(&self) -> Network {
        Connector::network(self.inner.as_ref())
    }

    fn capabilities(&self) -> Capabilities {
        Connector::capabilities(self.inner.as_ref())
    }

    async fn begin_login(&self) -> LoginStep {
        self.inner.begin_login().await
    }

    async fn submit_login(&self, input: LoginInput) -> LoginStep {
        self.inner.submit_login(input).await
    }

    async fn restore(&self, session: SessionBlob) -> Result<RemoteUser, ConnectorError> {
        self.inner.restore(session).await
    }

    async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError> {
        self.inner.start(events).await
    }

    async fn stop(&self) -> Result<(), ConnectorError> {
        self.inner.stop().await
    }

    async fn list_chats(
        &self,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteChat>, ConnectorError> {
        self.inner.list_chats(cursor, limit).await
    }

    async fn list_members(
        &self,
        remote_chat_id: &str,
    ) -> Result<Vec<RemoteMember>, ConnectorError> {
        self.inner.list_members(remote_chat_id).await
    }

    async fn backfill(
        &self,
        remote_chat_id: &str,
        before: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError> {
        if let Some(error) = self.backfill_error.lock().expect("backfill lock").clone() {
            return Err(error);
        }
        self.inner.backfill(remote_chat_id, before, limit).await
    }

    async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError> {
        self.inner.send(message).await
    }

    async fn edit(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        body: String,
        formatted: Option<String>,
    ) -> Result<(), ConnectorError> {
        self.inner
            .edit(remote_chat_id, remote_msg_id, body, formatted)
            .await
    }

    async fn delete(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        for_everyone: bool,
    ) -> Result<(), ConnectorError> {
        self.inner
            .delete(remote_chat_id, remote_msg_id, for_everyone)
            .await
    }

    async fn react(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        self.inner.react(remote_chat_id, remote_msg_id, emoji).await
    }

    async fn unreact(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        self.inner
            .unreact(remote_chat_id, remote_msg_id, emoji)
            .await
    }

    async fn mark_read(
        &self,
        remote_chat_id: &str,
        up_to_remote_msg_id: &str,
    ) -> Result<(), ConnectorError> {
        self.inner
            .mark_read(remote_chat_id, up_to_remote_msg_id)
            .await
    }

    async fn set_typing(&self, remote_chat_id: &str, typing: bool) -> Result<(), ConnectorError> {
        self.inner.set_typing(remote_chat_id, typing).await
    }

    async fn fetch_media(&self, media: &MediaRef, dest_path: &Path) -> Result<u64, ConnectorError> {
        self.inner.fetch_media(media, dest_path).await
    }

    async fn logout(&self) -> Result<(), ConnectorError> {
        self.inner.logout().await
    }
}

/// A fake connector with a finished login, so `list_chats` is allowed.
async fn logged_in_fake() -> Arc<FakeConnector> {
    let fake = Arc::new(FakeConnector::new());
    assert!(matches!(fake.begin_login().await, LoginStep::PhoneNumber));
    let step = fake
        .submit_login(LoginInput::PhoneNumber {
            phone: "+15550100".to_string(),
        })
        .await;
    assert!(matches!(step, LoginStep::Code { .. }));
    let step = fake
        .submit_login(LoginInput::Code {
            code: FAKE_CODE.to_string(),
        })
        .await;
    assert!(matches!(step, LoginStep::Done { .. }), "{step:?}");
    fake
}

struct Harness {
    engine: AppEngine,
    ctx: EngineContext,
    store: Store,
    dir: TempDir,
    account_id: AccountId,
}

/// Starts one account on a file-backed store so the test can inspect what was written.
async fn harness(connector: Arc<dyn Connector>, history: HistoryCapabilities) -> Harness {
    let dir = TempDir::new().expect("temp dir");
    let store = Store::open(dir.path().join("alloy.db"), KEY).expect("open the store");
    let record: AccountRecord = store
        .insert_account(NewAccount {
            account_id: None,
            network: StoreNetwork::Fake,
            remote_user_id: Some("fake-self".to_string()),
            display_name: Some("Test".to_string()),
            state: AccountState::Active,
        })
        .await
        .expect("store the account");
    let ctx = EngineContext::with_store(
        store.clone(),
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
        Arc::new(UnusedFactory),
    );
    let runtime = start_with_connector(&ctx, record.clone(), connector, None, history)
        .await
        .expect("start the account");
    runtime.abort_tasks();
    Harness {
        engine: AppEngine::new(ctx.clone()),
        ctx,
        store,
        dir,
        account_id: AccountId(record.account_id),
    }
}

async fn enrol(harness: &Harness) -> HistoryEnrolmentOutcome {
    match harness
        .engine
        .handle(EngineCommand::EnrolHistory {
            account_id: harness.account_id.clone(),
            pin: HistoryPin::new(PIN),
        })
        .await
        .expect("enrol history")
    {
        EngineResponse::HistoryEnrolment { outcome } => outcome,
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Every byte the store wrote, database and WAL alike.
fn store_bytes(dir: &Path) -> Vec<Vec<u8>> {
    ["alloy.db", "alloy.db-wal"]
        .iter()
        .filter_map(|name| std::fs::read(dir.join(name)).ok())
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn enrolling_calls_the_connector_once_and_stores_no_pin() {
    let capture = Arc::new(LogCapture::new());
    let _guard = tracing::subscriber::set_default(capture.clone());
    let fake = logged_in_fake().await;
    fake.set_history(true, false).await;
    let harness = harness(fake.clone(), HistoryCapabilities::default()).await;

    assert_eq!(enrol(&harness).await, HistoryEnrolmentOutcome::Enrolled);

    assert_eq!(
        fake.history_enrolment_calls().await,
        1,
        "the PIN is handed to the connector exactly once"
    );
    // Nothing was sealed as a connector secret: the enrolment lives in the sidecar's own blob.
    assert_eq!(
        harness
            .store
            .get_secret(&harness.account_id.0)
            .await
            .expect("read the secret row"),
        None,
        "enrolment must not write an account secret"
    );
    assert_eq!(
        harness
            .store
            .get_history_capabilities(&harness.account_id.0)
            .await
            .expect("read the archive flags"),
        alloy_store::HistoryCapabilities {
            pin_backup: true,
            enrolled: true,
        }
    );

    // Neither the database nor its WAL holds the PIN, and no log field does either.
    for bytes in store_bytes(harness.dir.path()) {
        assert!(
            !contains(&bytes, PIN.as_bytes()),
            "the store contains the archive PIN"
        );
    }
    let logs = capture.snapshot();
    assert!(
        !logs.iter().any(|(_, fields)| fields.contains(PIN)),
        "a log line leaked the archive PIN: {logs:?}"
    );
    assert!(
        !logs.iter().any(|(level, _)| *level == Level::WARN),
        "a successful enrolment warned: {logs:?}"
    );
}

#[tokio::test]
async fn a_rejected_pin_surfaces_the_attempts_left() {
    let fake = logged_in_fake().await;
    fake.set_history(true, false).await;
    fake.fail_history_enrolment(Some(ConnectorError::PinRejected {
        attempts_remaining: 3,
    }))
    .await;
    let harness = harness(fake, HistoryCapabilities::default()).await;

    assert_eq!(
        enrol(&harness).await,
        HistoryEnrolmentOutcome::PinRejected {
            attempts_remaining: 3
        }
    );
}

#[tokio::test]
async fn an_account_without_a_backup_is_a_capability_not_a_warning() {
    let capture = Arc::new(LogCapture::new());
    let _guard = tracing::subscriber::set_default(capture.clone());
    let fake = logged_in_fake().await;
    fake.set_history(true, false).await;
    fake.fail_history_enrolment(Some(ConnectorError::PinBackupUnavailable))
        .await;
    let harness = harness(fake, HistoryCapabilities::default()).await;

    assert_eq!(
        enrol(&harness).await,
        HistoryEnrolmentOutcome::PinBackupUnavailable
    );
    assert!(
        !capture
            .snapshot()
            .iter()
            .any(|(level, _)| *level == Level::WARN),
        "an account without an encrypted backup must not warn"
    );
    // The capability is corrected, so the UI stops offering a PIN that cannot work.
    assert_eq!(
        harness
            .store
            .get_history_capabilities(&harness.account_id.0)
            .await
            .expect("read the archive flags"),
        alloy_store::HistoryCapabilities {
            pin_backup: false,
            enrolled: false,
        }
    );
}

#[tokio::test]
async fn paging_says_an_unread_backup_is_not_enrolled_rather_than_the_start_of_the_chat() {
    let fake = logged_in_fake().await;
    let connector = Arc::new(OlderConnector::new(fake));
    // The account has a backup; this device simply has not entered its PIN yet.
    let harness = harness(
        connector.clone(),
        HistoryCapabilities {
            pin_backup: true,
            enrolled: false,
        },
    )
    .await;
    // The setup's own sync stored real history; from here the archive refuses.
    connector.refuse_backfill(ConnectorError::NotSupported {
        feature: "backfill".to_string(),
    });
    let chat = harness
        .store
        .list_chats(Some(harness.account_id.0.clone()), false, 1)
        .await
        .expect("list the synced chats")
        .pop()
        .expect("a chat from the initial sync");
    let room = alloy_protocol::RoomId(chat.chat_id);
    alloy_engine::projection::subscribe_timeline(&harness.ctx, &room)
        .await
        .expect("open the timeline");

    // Page until the local window is exhausted; the network answer is the reason paging stopped.
    let mut reason = None;
    for _ in 0..50 {
        match alloy_engine::projection::paginate_backwards(&harness.ctx, &room, 10)
            .await
            .expect("page history")
        {
            Some(limit) => {
                reason = Some(limit);
                break;
            }
            None => continue,
        }
    }
    assert_eq!(
        reason,
        Some(alloy_protocol::HistoryLimit::NotEnrolled),
        "a refusal from an account with an unenrolled backup must not read as the start of the chat"
    );
}

#[tokio::test]
async fn an_older_connector_answers_not_supported_without_a_protocol_break() {
    let fake = logged_in_fake().await;
    let older: Arc<dyn Connector> = Arc::new(OlderConnector::new(fake.clone()));
    let harness = harness(older, HistoryCapabilities::default()).await;

    assert_eq!(enrol(&harness).await, HistoryEnrolmentOutcome::NotSupported);
    assert_eq!(
        fake.history_enrolment_calls().await,
        0,
        "the default body never reaches an older connector"
    );
}
