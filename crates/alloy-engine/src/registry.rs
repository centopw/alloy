use alloy_protocol::{RoomId, SubscriptionId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

/// One tracked background task and the room it belongs to, when it is scoped to one room.
#[derive(Debug)]
struct Subscription {
    handle: AbortHandle,
    room: Option<RoomId>,
}

/// Tracks active background subscription tasks by `SubscriptionId`.
///
/// Cancelling a subscription aborts its corresponding background task immediately. Subscriptions
/// that are scoped to a room are also reachable by room, so closing a room can stop all of them.
#[derive(Clone, Default, Debug)]
pub struct SubscriptionRegistry {
    tasks: Arc<RwLock<HashMap<SubscriptionId, Subscription>>>,
}

impl SubscriptionRegistry {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a subscription with an explicit `AbortHandle`.
    pub async fn register(&self, id: SubscriptionId, handle: AbortHandle) {
        self.register_scoped(id, None, handle).await;
    }

    /// Registers a subscription that only makes sense while `room_id` is open, so closing that room
    /// can abort it.
    pub async fn register_for_room(
        &self,
        id: SubscriptionId,
        room_id: RoomId,
        handle: AbortHandle,
    ) {
        self.register_scoped(id, Some(room_id), handle).await;
    }

    async fn register_scoped(&self, id: SubscriptionId, room: Option<RoomId>, handle: AbortHandle) {
        let mut map = self.tasks.write().await;
        // If there was an existing handle with this ID, abort it first.
        if let Some(old) = map.insert(id, Subscription { handle, room }) {
            old.handle.abort();
        }
    }

    /// Cancels and removes a subscription by ID, aborting its background task.
    /// Returns `true` if a subscription existed and was aborted.
    pub async fn cancel(&self, id: &SubscriptionId) -> bool {
        let mut map = self.tasks.write().await;
        if let Some(subscription) = map.remove(id) {
            subscription.handle.abort();
            true
        } else {
            false
        }
    }

    /// Aborts every subscription scoped to `room_id` and returns how many were aborted.
    pub async fn cancel_room(&self, room_id: &RoomId) -> usize {
        let mut map = self.tasks.write().await;
        let matching: Vec<SubscriptionId> = map
            .iter()
            .filter(|(_, subscription)| subscription.room.as_ref() == Some(room_id))
            .map(|(id, _)| id.clone())
            .collect();
        let mut aborted = 0;
        for id in matching {
            if let Some(subscription) = map.remove(&id) {
                subscription.handle.abort();
                aborted += 1;
            }
        }
        aborted
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
        for (_, subscription) in map.drain() {
            subscription.handle.abort();
        }
    }
}
