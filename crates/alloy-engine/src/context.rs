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
    pub client: Arc<RwLock<Option<AlloyClient>>>,
    pub sink: EventSink,
    pub data_dir: PathBuf,
    pub secrets: Arc<dyn SecretStore>,
    pub subscriptions: SubscriptionRegistry,
}

impl EngineContext {
    pub fn new(data_dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            sink: EventSink::new(),
            data_dir,
            secrets,
            subscriptions: SubscriptionRegistry::new(),
        }
    }
}
