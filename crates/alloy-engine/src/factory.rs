//! Which networks this build can connect, and how a connector is built for one of them.
//!
//! The engine never constructs a network connector itself: it asks the factory injected into
//! [`EngineContext`](crate::context::EngineContext). A desktop build injects
//! [`DefaultConnectorFactory`]; the end-to-end tests inject [`FakeConnectorFactory`].
//!
//! The Telegram application pair is the person's own setting: the app asks for it once and seals
//! it in the OS keyring ([`TELEGRAM_APP_SECRET`]). Nothing is compiled in, so an installed binary
//! carries no credential, and the pair can be replaced without a release. The process environment
//! stays as a developer override.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use alloy_connector::{Connector, ConnectorError};
use alloy_protocol::{Network, NetworkInfoDto};
use alloy_sidecar::{SidecarConfig, SidecarConnector};
use alloy_storage::{SecretStore, StorageError};
use alloy_telegram::{ConfigError, TelegramConfig, TelegramConnector};
use serde::{Deserialize, Serialize};

use crate::convert;
use crate::error::EngineError;

/// Environment variable naming the Messenger sidecar executable.
pub const MESSENGER_SIDECAR_ENV: &str = "ALLOY_MESSENGER_SIDECAR";

/// Secret under which the user-entered Telegram application pair is sealed.
pub const TELEGRAM_APP_SECRET: &str = "alloy.telegram.app.v1";

/// Named in [`ConnectorError::NotSupported`] when Telegram has no application pair yet.
pub const TELEGRAM_APP_FEATURE: &str = "telegram application credentials";

/// Length of a Telegram `api_hash`: 32 hexadecimal characters.
const API_HASH_LEN: usize = 32;

/// The application pair as the user entered it, sealed as JSON under [`TELEGRAM_APP_SECRET`].
///
/// The wire shape is `{"api_id":<i32>,"api_hash":"<hex>"}`. [`fmt::Debug`] redacts the hash
/// because it is a credential.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramAppRecord {
    /// `api_id` from <https://my.telegram.org/apps>.
    pub api_id: i32,
    /// `api_hash` paired with [`Self::api_id`]. Never log or return it.
    pub api_hash: String,
}

impl TelegramAppRecord {
    /// Validates a user-entered pair: a positive `api_id` and a 32-character hex `api_hash`.
    ///
    /// The message is returned as text so the caller decides the error code; it never contains
    /// the hash.
    pub fn parse(api_id: i32, api_hash: &str) -> Result<Self, String> {
        if api_id <= 0 {
            return Err(format!(
                "the Telegram api_id must be a positive number, got {api_id}"
            ));
        }
        let api_hash = api_hash.trim();
        if api_hash.len() != API_HASH_LEN || !api_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "the Telegram api_hash must be the {API_HASH_LEN}-character hexadecimal value \
                 shown at https://my.telegram.org/apps"
            ));
        }
        Ok(Self {
            api_id,
            api_hash: api_hash.to_string(),
        })
    }

    /// The connector configuration for this pair, with Alloy's client defaults.
    pub fn config(&self) -> TelegramConfig {
        TelegramConfig::new(self.api_id, self.api_hash.clone())
    }
}

impl fmt::Debug for TelegramAppRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramAppRecord")
            .field("api_id", &self.api_id)
            .field("api_hash", &"<redacted>")
            .finish()
    }
}

/// Where the Telegram application pair in effect came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramAppSource {
    /// Entered in the app and sealed in the OS keyring.
    Runtime,
    /// Exported as `ALLOY_TELEGRAM_API_ID`/`ALLOY_TELEGRAM_API_HASH` for a development build.
    Environment,
}

/// A usable Telegram application pair and where it came from.
#[derive(Debug, Clone)]
pub struct ResolvedTelegramApp {
    pub config: TelegramConfig,
    pub source: TelegramAppSource,
}

/// Resolves the pair per call: the sealed runtime value first, then [`TelegramConfig::from_env`],
/// then none.
///
/// A sealed value that cannot be decoded is logged and treated as absent, so a corrupt keyring
/// entry never stops the app; entering the pair again overwrites it.
pub async fn resolve_telegram_app(secrets: &dyn SecretStore) -> Option<ResolvedTelegramApp> {
    match read_telegram_app(secrets).await {
        Ok(Some(record)) => {
            return Some(ResolvedTelegramApp {
                config: record.config(),
                source: TelegramAppSource::Runtime,
            });
        }
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(error = %err.message, "cannot read the sealed Telegram application pair")
        }
    }
    match TelegramConfig::from_env() {
        Ok(config) => Some(ResolvedTelegramApp {
            config,
            source: TelegramAppSource::Environment,
        }),
        Err(err @ ConfigError::NotConfigured) => {
            tracing::debug!(error = %err, "no Telegram application pair is configured");
            None
        }
        Err(err) => {
            tracing::warn!(error = %err, "Telegram credentials in the environment are unusable");
            None
        }
    }
}

/// Reads the sealed pair; `Ok(None)` when none is stored.
pub async fn read_telegram_app(
    secrets: &dyn SecretStore,
) -> Result<Option<TelegramAppRecord>, EngineError> {
    match secrets.get_secret(TELEGRAM_APP_SECRET).await {
        Ok(json) => serde_json::from_str(&json).map(Some).map_err(|err| {
            EngineError::internal_error(format!(
                "the sealed Telegram application pair is not valid JSON: {err}"
            ))
        }),
        Err(StorageError::SecretNotFound(_)) => Ok(None),
        Err(err) => Err(EngineError::internal_error(format!(
            "cannot read the sealed Telegram application pair: {err}"
        ))),
    }
}

/// Seals `record` in the OS keyring, replacing any previous pair.
pub async fn save_telegram_app(
    secrets: &dyn SecretStore,
    record: &TelegramAppRecord,
) -> Result<(), EngineError> {
    let json = serde_json::to_string(record).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot encode the Telegram application pair: {err}"
        ))
    })?;
    secrets
        .set_secret(TELEGRAM_APP_SECRET, &json)
        .await
        .map_err(|err| {
            EngineError::internal_error(format!("cannot seal the Telegram application pair: {err}"))
        })
}

/// Deletes the sealed pair; a later login needs it entered again.
pub async fn clear_telegram_app(secrets: &dyn SecretStore) -> Result<(), EngineError> {
    match secrets.delete_secret(TELEGRAM_APP_SECRET).await {
        Ok(()) | Err(StorageError::SecretNotFound(_)) => Ok(()),
        Err(err) => Err(EngineError::internal_error(format!(
            "cannot delete the sealed Telegram application pair: {err}"
        ))),
    }
}

/// The error a login gets when Telegram has no application pair yet.
///
/// `not_supported` with [`TELEGRAM_APP_FEATURE`] as the reason, and a message that names where the
/// user enters the pair, because this is a setting the person can fix without a new release.
pub fn telegram_app_missing_error() -> EngineError {
    EngineError {
        code: "not_supported".to_string(),
        message: "Telegram needs its application pair first: enter the api_id and api_hash from \
                  https://my.telegram.org/apps in Settings -> Networks -> Telegram application"
            .to_string(),
        reason: Some(TELEGRAM_APP_FEATURE.to_string()),
    }
}

/// Builds a connector for one network in one account's private directory.
#[async_trait::async_trait]
pub trait ConnectorFactory: Send + Sync {
    /// Networks this factory can offer, in the order `ListNetworks` reports them.
    ///
    /// Telegram is always present; `ready` and `setup` say whether a login can start now.
    async fn networks(&self) -> Vec<NetworkInfoDto>;

    /// Creates a connector whose own files may live in `account_dir` (created by the caller).
    async fn create(
        &self,
        network: Network,
        account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError>;
}

/// The production factory: in-process Telegram and, when installed, the Messenger sidecar.
///
/// It holds no credential of its own: the Telegram pair is read from `secrets` on every call, so
/// a pair entered while the app runs takes effect without a restart.
#[derive(Clone)]
pub struct DefaultConnectorFactory {
    /// The platform secret store the Telegram application pair is sealed in.
    pub secrets: Arc<dyn SecretStore>,
    /// Executable speaking the connector protocol for Messenger; `None` omits Messenger.
    pub messenger_sidecar: Option<PathBuf>,
}

impl DefaultConnectorFactory {
    pub fn new(secrets: Arc<dyn SecretStore>, messenger_sidecar: Option<PathBuf>) -> Self {
        Self {
            secrets,
            messenger_sidecar,
        }
    }

    /// Reads the build's configuration: the Messenger sidecar from [`MESSENGER_SIDECAR_ENV`]. A
    /// missing sidecar only removes that network; it is not an error.
    pub fn from_env(secrets: Arc<dyn SecretStore>) -> Self {
        let messenger_sidecar = std::env::var_os(MESSENGER_SIDECAR_ENV).map(PathBuf::from);
        Self {
            secrets,
            messenger_sidecar,
        }
    }

    /// The Telegram configuration in effect for this call, or the error a login gets when the user
    /// has not entered a pair yet.
    pub async fn telegram_config(&self) -> Result<TelegramConfig, ConnectorError> {
        resolve_telegram_app(self.secrets.as_ref())
            .await
            .map(|resolved| resolved.config)
            .ok_or_else(|| ConnectorError::NotSupported {
                feature: TELEGRAM_APP_FEATURE.to_string(),
            })
    }
}

#[async_trait::async_trait]
impl ConnectorFactory for DefaultConnectorFactory {
    async fn networks(&self) -> Vec<NetworkInfoDto> {
        let telegram = resolve_telegram_app(self.secrets.as_ref()).await;
        let mut networks = vec![convert::telegram_network_info(telegram.is_some())];
        if self.messenger_sidecar.is_some() {
            networks.push(convert::messenger_network_info());
        }
        networks
    }

    async fn create(
        &self,
        network: Network,
        account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError> {
        match network {
            Network::Telegram => {
                let config = self.telegram_config().await?;
                Ok(Arc::new(TelegramConnector::new(config)))
            }
            Network::Messenger => {
                let program =
                    self.messenger_sidecar
                        .clone()
                        .ok_or_else(|| ConnectorError::NotSupported {
                            feature: convert::network_label(network).to_string(),
                        })?;
                let connector =
                    SidecarConnector::spawn(SidecarConfig::new(program, account_dir)).await?;
                Ok(Arc::new(connector))
            }
            Network::Fake => Err(ConnectorError::NotSupported {
                feature: convert::network_label(network).to_string(),
            }),
        }
    }
}

#[cfg(feature = "testing")]
pub use testing::{CapabilityOverrideConnector, FakeConnectorFactory, SealedTelegramFactory};

#[cfg(feature = "testing")]
mod testing {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, PoisonError};

    use alloy_connector::capabilities::Capabilities;
    use alloy_connector::fake::FakeConnector;
    use alloy_connector::{
        Connector, ConnectorError, ConnectorEvent, LoginInput, LoginStep, MediaRef,
        OutgoingMessage, Page, RemoteChat, RemoteMessage, RemoteUser, SendAck, SessionBlob,
    };
    use alloy_protocol::{Network, NetworkInfoDto};
    use alloy_storage::SecretStore;
    use async_trait::async_trait;
    use tokio::sync::mpsc::Sender;

    use super::{ConnectorFactory, DefaultConnectorFactory};
    use crate::convert::network_label;

    /// Adjusts the capabilities a fake connector reports before the engine sees them.
    pub type CapabilityOverride = Arc<dyn Fn(&mut Capabilities) + Send + Sync>;

    /// A factory that builds deterministic in-memory connectors for [`Network::Fake`].
    ///
    /// Tests keep a handle to every fake it created: the last one is the connector behind the
    /// current account, so a test can inject an incoming message without a network.
    #[derive(Default)]
    pub struct FakeConnectorFactory {
        created: Mutex<Vec<Arc<FakeConnector>>>,
        capability_override: Option<CapabilityOverride>,
    }

    impl FakeConnectorFactory {
        pub fn new() -> Self {
            Self::default()
        }

        /// Adjusts the capabilities every fake reports, so capability gates can be exercised on a
        /// network that really lacks a feature.
        pub fn with_capability_override(
            apply: impl Fn(&mut Capabilities) + Send + Sync + 'static,
        ) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                capability_override: Some(Arc::new(apply)),
            }
        }

        /// The connector created most recently, which is the one the engine is driving.
        pub fn last(&self) -> Option<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .last()
                .cloned()
        }

        /// Every connector created so far, oldest first.
        pub fn all(&self) -> Vec<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl ConnectorFactory for FakeConnectorFactory {
        async fn networks(&self) -> Vec<NetworkInfoDto> {
            vec![NetworkInfoDto {
                network: Network::Fake,
                ready: true,
                setup: None,
            }]
        }

        async fn create(
            &self,
            network: Network,
            _account_dir: PathBuf,
        ) -> Result<Arc<dyn Connector>, ConnectorError> {
            if network != Network::Fake {
                return Err(ConnectorError::NotSupported {
                    feature: network_label(network).to_string(),
                });
            }
            let fake = Arc::new(FakeConnector::new());
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fake.clone());
            match &self.capability_override {
                Some(apply) => {
                    let mut capabilities = fake.capabilities();
                    apply(&mut capabilities);
                    let connector: Arc<dyn Connector> = Arc::new(CapabilityOverrideConnector {
                        inner: fake,
                        capabilities,
                    });
                    Ok(connector)
                }
                None => {
                    let connector: Arc<dyn Connector> = fake;
                    Ok(connector)
                }
            }
        }
    }

    /// A factory that keeps the production Telegram credential gate and swaps the connector for
    /// the deterministic fake, so a test can prove a login reaches a connector without a network.
    ///
    /// `networks()` is exactly [`DefaultConnectorFactory`]'s, so readiness and the setup block
    /// under test are the production ones.
    pub struct SealedTelegramFactory {
        factory: DefaultConnectorFactory,
        created: Mutex<Vec<Arc<FakeConnector>>>,
    }

    impl SealedTelegramFactory {
        pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
            Self {
                factory: DefaultConnectorFactory::new(secrets, None),
                created: Mutex::new(Vec::new()),
            }
        }

        /// The connector created most recently.
        pub fn last(&self) -> Option<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .last()
                .cloned()
        }
    }

    #[async_trait]
    impl ConnectorFactory for SealedTelegramFactory {
        async fn networks(&self) -> Vec<NetworkInfoDto> {
            self.factory.networks().await
        }

        async fn create(
            &self,
            network: Network,
            _account_dir: PathBuf,
        ) -> Result<Arc<dyn Connector>, ConnectorError> {
            if network != Network::Telegram {
                return Err(ConnectorError::NotSupported {
                    feature: network_label(network).to_string(),
                });
            }
            // The production gate: no pair sealed and none in the environment.
            self.factory.telegram_config().await?;
            let fake = Arc::new(FakeConnector::new());
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fake.clone());
            Ok(fake)
        }
    }

    /// Delegates every call to a real connector while reporting adjusted capabilities.
    ///
    /// Only for tests: it makes a capability observable as absent without a second connector
    /// implementation whose behaviour would drift from the fake's.
    pub struct CapabilityOverrideConnector {
        inner: Arc<FakeConnector>,
        capabilities: Capabilities,
    }

    #[async_trait]
    impl Connector for CapabilityOverrideConnector {
        fn network(&self) -> alloy_connector::Network {
            self.inner.network()
        }

        fn capabilities(&self) -> Capabilities {
            self.capabilities.clone()
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
            remote_chat_id: &str,
            before: Option<String>,
            limit: usize,
        ) -> Result<Page<RemoteMessage>, ConnectorError> {
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

        async fn set_typing(
            &self,
            remote_chat_id: &str,
            typing: bool,
        ) -> Result<(), ConnectorError> {
            self.inner.set_typing(remote_chat_id, typing).await
        }

        async fn fetch_media(
            &self,
            media: &MediaRef,
            dest_path: &Path,
        ) -> Result<u64, ConnectorError> {
            self.inner.fetch_media(media, dest_path).await
        }

        async fn logout(&self) -> Result<(), ConnectorError> {
            self.inner.logout().await
        }
    }
}
