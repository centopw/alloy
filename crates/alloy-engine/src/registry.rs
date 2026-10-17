use alloy_protocol::SubscriptionId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

/// Tracks active background subscription tasks by `SubscriptionId`.
///
/// Cancelling a subscription aborts its corresponding background task immediately.
#[derive(Clone, Default, Debug)]
pub struct SubscriptionRegistry {
    tasks: Arc<RwLock<HashMap<SubscriptionId, AbortHandle>>>,
}

impl SubscriptionRegistry {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a subscription with an explicit `AbortHandle`.
    pub async fn register(&self, id: SubscriptionId, handle: AbortHandle) {
        let mut map = self.tasks.write().await;
        // If there was an existing handle with this ID, abort it first.
        if let Some(old) = map.insert(id, handle) {
            old.abort();
        }
    }

    /// Cancels and removes a subscription by ID, aborting its background task.
    /// Returns `true` if a subscription existed and was aborted.
    pub async fn cancel(&self, id: &SubscriptionId) -> bool {
        let mut map = self.tasks.write().await;
        if let Some(handle) = map.remove(id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    /// Checks if a subscription is currently active in the registry.
    pub async fn is_active(&self, id: &SubscriptionId) -> bool {
        let map = self.tasks.read().await;
        map.contains_key(id)
    }

    /// Returns the number of currently tracked active subscriptions.
    pub async fn active_count(&self) -> usize {
        let map = self.tasks.read().await;
        map.len()
    }

    /// Aborts all active subscriptions and empties the registry.
    pub async fn clear(&self) {
        let mut map = self.tasks.write().await;
        for (_, handle) in map.drain() {
            handle.abort();
        }
    }
}
