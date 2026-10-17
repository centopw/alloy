use alloy_protocol::EngineEvent;
use tokio::sync::broadcast;

const DEFAULT_CAPACITY: usize = 1024;

/// Clone-able event sink wrapping a tokio broadcast channel for fan-out.
#[derive(Clone, Debug)]
pub struct EventSink {
    sender: broadcast::Sender<EngineEvent>,
}

impl EventSink {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Emits an event to all active subscribers.
    ///
    /// If there are no receivers, the send error is ignored.
    pub fn emit(&self, event: EngineEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribes to the broadcast stream, returning a new `broadcast::Receiver`.
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.sender.subscribe()
    }

    /// Returns the number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventSink {
    fn default() -> Self {
        Self::new()
    }
}
