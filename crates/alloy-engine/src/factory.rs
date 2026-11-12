//! Which networks this build can connect, and how a connector is built for one of them.
//!
//! The engine never constructs a network connector itself: it asks the factory injected into
//! [`EngineContext`](crate::context::EngineContext). A desktop build injects
//! [`DefaultConnectorFactory`]; the end-to-end tests inject [`FakeConnectorFactory`].

use std::path::PathBuf;
use std::sync::Arc;

use alloy_connector::{Connector, ConnectorError};
use alloy_protocol::Network;
use alloy_sidecar::{SidecarConfig, SidecarConnector};
use alloy_telegram::{ConfigError, TelegramConfig, TelegramConnector};

use crate::convert::network_label;

/// Environment variable naming the Messenger sidecar executable.
pub const MESSENGER_SIDECAR_ENV: &str = "ALLOY_MESSENGER_SIDECAR";

/// Builds a connector for one network in one account's private directory.
#[async_trait::async_trait]
pub trait ConnectorFactory: Send + Sync {
    /// Networks this factory can actually connect, in the order `ListNetworks` reports them.
    fn networks(&self) -> Vec<Network>;

    /// Creates a connector whose own files may live in `account_dir` (created by the caller).
    async fn create(
        &self,
        network: Network,
        account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError>;
}

/// The production factory: in-process Telegram and, when installed, the Messenger sidecar.
#[derive(Debug, Clone, Default)]
pub struct DefaultConnectorFactory {
    /// Telegram application credentials; `None` omits Telegram from the network list.
    pub telegram: Option<TelegramConfig>,
    /// Executable speaking the connector protocol for Messenger; `None` omits Messenger.
    pub messenger_sidecar: Option<PathBuf>,
}

impl DefaultConnectorFactory {
    pub fn new(telegram: Option<TelegramConfig>, messenger_sidecar: Option<PathBuf>) -> Self {
        Self {
            telegram,
            messenger_sidecar,
        }
    }

    /// Reads the build's configuration: the Telegram application pair through
    /// [`TelegramConfig::resolve`] and the Messenger sidecar from [`MESSENGER_SIDECAR_ENV`]. A
    /// missing piece only removes that network; it is not an error, so a build with neither still
    /// starts and lists no networks.
    pub fn from_env() -> Self {
        let telegram = match TelegramConfig::resolve() {
            Ok(config) => Some(config),
            // Nothing is wrong with an install that lacks the pair, and nobody using the app can
            // set it: the error carries the instructions the developer building it needs.
            Err(err @ ConfigError::NotConfigured) => {
                tracing::info!("{err}");
                None
            }
            Err(err) => {
                tracing::warn!(error = %err, "Telegram credentials are misconfigured");
                None
            }
        };
        let messenger_sidecar = std::env::var_os(MESSENGER_SIDECAR_ENV).map(PathBuf::from);
        Self {
            telegram,
            messenger_sidecar,
        }
    }
}

#[async_trait::async_trait]
impl ConnectorFactory for DefaultConnectorFactory {
    fn networks(&self) -> Vec<Network> {
        let mut networks = Vec::new();
        if self.telegram.is_some() {
            networks.push(Network::Telegram);
        }
        if self.messenger_sidecar.is_some() {
            networks.push(Network::Messenger);
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
                let config = self
                    .telegram
                    .clone()
                    .ok_or_else(|| ConnectorError::NotSupported {
                        feature: network_label(network).to_string(),
                    })?;
                Ok(Arc::new(TelegramConnector::new(config)))
            }
            Network::Messenger => {
                let program =
                    self.messenger_sidecar
                        .clone()
                        .ok_or_else(|| ConnectorError::NotSupported {
                            feature: network_label(network).to_string(),
                        })?;
                let connector =
                    SidecarConnector::spawn(SidecarConfig::new(program, account_dir)).await?;
                Ok(Arc::new(connector))
            }
            Network::Fake => Err(ConnectorError::NotSupported {
                feature: network_label(network).to_string(),
            }),
        }
    }
}

#[cfg(feature = "testing")]
pub use testing::{CapabilityOverrideConnector, FakeConnectorFactory};

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
    use alloy_protocol::Network;
    use async_trait::async_trait;
    use tokio::sync::mpsc::Sender;

    use super::ConnectorFactory;
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
        fn networks(&self) -> Vec<Network> {
            vec![Network::Fake]
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
