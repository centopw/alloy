//! Startup history backfill must not report a missing capability as a failure.
//!
//! The rule (see `docs/TRACKING.md` and the decided log policy) is that a network which cannot
//! backfill a chat is routine, not an error: `NotSupported` logs at `debug`, and only a real
//! failure logs at `warn`. The paging path in `projection.rs` already follows it; these tests pin
//! the startup loop in `accounts.rs`.
//!
//! The connector seam is the deterministic fake wrapped in a delegate that answers the configured
//! error from `backfill` alone. Log levels are observed with a hand-written `tracing::Subscriber`
//! installed as the thread-local default, because `alloy-engine` has no `tracing-subscriber`
//! dependency to build a test writer from; the subscriber records the level and rendered fields of
//! every event the startup sync emits.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alloy_connector::fake::{FAKE_CODE, FakeConnector};
use alloy_connector::{
    Capabilities, Connector, ConnectorError, ConnectorEvent, LoginInput, LoginStep, MediaRef,
    Network, OutgoingMessage, Page, RemoteChat, RemoteMessage, RemoteUser, SendAck, SessionBlob,
};
use alloy_engine::EngineContext;
use alloy_engine::accounts::start_with_connector;
use alloy_engine::factory::ConnectorFactory;
use alloy_protocol::NetworkInfoDto;
use alloy_storage::InMemorySecretStore;
use alloy_store::{AccountRecord, AccountState, Network as StoreNetwork, NewAccount, Store};
use tempfile::TempDir;
use tokio::sync::mpsc::Sender;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Level, Metadata, Subscriber};

const KEY: [u8; 32] = [7; 32];

/// Records the level and rendered fields of every event dispatched to it.
#[derive(Default)]
struct LogCapture {
    events: Mutex<Vec<(Level, String)>>,
}

impl LogCapture {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, level: Level, fields: String) {
        self.events
            .lock()
            .expect("log capture lock")
            .push((level, fields));
    }

    fn snapshot(&self) -> Vec<(Level, String)> {
        self.events.lock().expect("log capture lock").clone()
    }
}

/// Renders an event's fields as `name=value` pairs.
#[derive(Default)]
struct RenderedFields(String);

impl Visit for RenderedFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        // Writing into a String cannot fail.
        let _ = write!(self.0, "{}={value:?}", field.name());
    }
}

impl Subscriber for LogCapture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        // Capture every level, so the test can prove `debug` replaced `warn` rather than both
        // being filtered out.
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        // Spans are not asserted on; a single shared id is enough for the visitor callbacks.
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = RenderedFields::default();
        event.record(&mut fields);
        self.record(*event.metadata().level(), fields.0);
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

/// The production factory's seam, unused by these tests: startup is driven directly with a
/// connector so `backfill` can be made to fail in a chosen way.
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

/// The deterministic fake, except that `backfill` always answers `error`.
///
/// Every other call is delegated, so the startup sync sees the fake's real chats and history
/// shape and only the history capability differs.
struct BackfillFails {
    inner: Arc<FakeConnector>,
    error: ConnectorError,
}

#[async_trait::async_trait]
impl Connector for BackfillFails {
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

    async fn backfill(
        &self,
        _remote_chat_id: &str,
        _before: Option<String>,
        _limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError> {
        Err(self.error.clone())
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

/// Runs one startup sync whose `backfill` always fails with `error` and returns every log event
/// the sync emitted.
async fn startup_logs(error: ConnectorError) -> Vec<(Level, String)> {
    let capture = Arc::new(LogCapture::new());
    // The test runtime is single-threaded (`#[tokio::test]`'s default), so the thread-local
    // subscriber stays installed for every poll of the sync.
    let _guard = tracing::subscriber::set_default(capture.clone());

    let dir = TempDir::new().expect("temp dir");
    let store = Store::open_in_memory(KEY).expect("open the store");
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
        store,
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
        Arc::new(UnusedFactory),
    );
    let connector: Arc<dyn Connector> = Arc::new(BackfillFails {
        inner: logged_in_fake().await,
        error,
    });

    let runtime = start_with_connector(
        &ctx,
        record,
        connector,
        None,
        alloy_connector::HistoryCapabilities::default(),
    )
    .await
    .expect("start the account");
    runtime.abort_tasks();

    capture.snapshot()
}

#[tokio::test]
async fn startup_backfill_of_an_unsupported_chat_logs_at_debug_only() {
    let logs = startup_logs(ConnectorError::NotSupported {
        feature: "backfill".to_string(),
    })
    .await;

    let warnings: Vec<&(Level, String)> = logs
        .iter()
        .filter(|(level, _)| *level == Level::WARN)
        .collect();
    assert!(
        warnings.is_empty(),
        "an unsupported capability is not a failure; got {warnings:?}"
    );

    let debugged: Vec<&str> = logs
        .iter()
        .filter(|(level, _)| *level == Level::DEBUG)
        .map(|(_, fields)| fields.as_str())
        .collect();
    assert!(
        debugged
            .iter()
            .any(|fields| fields.contains("no history to backfill") && fields.contains("chat=")),
        "the startup loop must say at debug which chat cannot be backfilled; got {debugged:?}"
    );
}

#[tokio::test]
async fn startup_backfill_of_a_real_failure_still_logs_a_warning() {
    let logs = startup_logs(ConnectorError::Network {
        message: "connection reset by peer".to_string(),
    })
    .await;

    assert!(
        logs.iter().any(|(level, fields)| *level == Level::WARN
            && fields.contains("history backfill failed")
            && fields.contains("connection reset by peer")),
        "a genuine failure must keep its warning; got {logs:?}"
    );
    assert!(
        !logs
            .iter()
            .any(|(_, fields)| fields.contains("no history to backfill")),
        "a real failure must not be excused as a missing capability; got {logs:?}"
    );
}
