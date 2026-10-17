use crate::registry::SubscriptionRegistry;
use crate::sink::EventSink;
use alloy_core::AlloyClient;
use alloy_storage::SecretStore;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared engine context passed to command handlers.
#[derive(Clone)]
pub struct EngineContext {
    pub client: Arc<RwLock<Option<Arc<AlloyClient>>>>,
    pub sink: EventSink,
    pub data_dir: PathBuf,
    pub secrets: Arc<dyn SecretStore>,
    pub subscriptions: SubscriptionRegistry,
    /// Start sync, recovery and verification watchers after login (disable for headless use).
    pub background: bool,
}

impl EngineContext {
    pub fn new(data_dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            sink: EventSink::new(),
            data_dir,
            secrets,
            subscriptions: SubscriptionRegistry::new(),
            background: true,
        }
    }

    /// Do not start background sync/watchers after login.
    pub fn without_background(mut self) -> Self {
        self.background = false;
        self
    }
}

impl EngineContext {
    /// The active client, or an `auth_failed` error when nobody is logged in.
    pub async fn active_client(&self) -> Result<Arc<AlloyClient>, alloy_protocol::EngineError> {
        self.client
            .read()
            .await
            .clone()
            .ok_or_else(|| alloy_protocol::EngineError::auth_failed("not logged in"))
    }

    /// Replace the active client, cancelling all subscriptions of the previous one.
    pub async fn set_client(&self, client: Option<AlloyClient>) {
        self.subscriptions.clear().await;
        let old = std::mem::replace(&mut *self.client.write().await, client.map(Arc::new));
        if let Some(old) = old {
            old.stop_sync().await;
        }
    }
}
