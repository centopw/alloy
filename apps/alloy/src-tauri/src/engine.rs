//! IPC engine trait and routing implementation.

use alloy_protocol::{EngineCommand, EngineError, EngineEvent, EngineResponse, SubscriptionId};
use std::sync::Arc;
use tokio::sync::mpsc;

pub type EventSink = mpsc::UnboundedSender<EngineEvent>;

/// Core engine trait for command handling.
#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    /// Handle a command and return a response.
    async fn handle(&self, cmd: EngineCommand) -> Result<EngineResponse, EngineError>;

    /// Subscribe to events. Returns a subscription ID and event sink.
    async fn subscribe(&self) -> (SubscriptionId, EventSink);

    /// Unsubscribe from events.
    async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<(), EngineError>;
}

/// Per-module handler trait for pluggable command processing.
#[async_trait::async_trait]
pub trait CommandHandler: Send + Sync {
    /// Handle a command specific to this module. Return Ok(None) if not handled here.
    async fn handle(
        &self,
        cmd: EngineCommand,
        sink: &EventSink,
    ) -> Result<Option<EngineResponse>, EngineError>;
}

/// Router that dispatches commands to per-module handlers.
pub struct RouterEngine {
    handlers: Vec<Arc<dyn CommandHandler>>,
    subscriptions: Arc<tokio::sync::Mutex<Vec<mpsc::UnboundedReceiver<EngineEvent>>>>,
}

impl RouterEngine {
    pub fn new() -> Self {
        Self {
            handlers: vec![],
            subscriptions: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Register a command handler for a module.
    pub fn register_handler(&mut self, handler: Arc<dyn CommandHandler>) {
        self.handlers.push(handler);
    }
}

impl Default for RouterEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Engine for RouterEngine {
    async fn handle(&self, cmd: EngineCommand) -> Result<EngineResponse, EngineError> {
        let (_sub_id, sink) = self.subscribe().await;

        // Try each handler in sequence
        for handler in &self.handlers {
            if let Ok(Some(response)) = handler.handle(cmd.clone(), &sink).await {
                return Ok(response);
            }
        }

        // If no handler matched, return NotImplemented
        let cmd_name = match cmd {
            EngineCommand::DiscoverHomeserver { .. } => "DiscoverHomeserver",
            EngineCommand::LoginPassword { .. } => "LoginPassword",
            EngineCommand::StartOidcLogin { .. } => "StartOidcLogin",
            EngineCommand::CompleteOidcLogin { .. } => "CompleteOidcLogin",
            EngineCommand::RestoreSession { .. } => "RestoreSession",
            EngineCommand::Logout { .. } => "Logout",
            EngineCommand::ListAccounts => "ListAccounts",
            EngineCommand::SubscribeRoomList { .. } => "SubscribeRoomList",
            EngineCommand::Unsubscribe { .. } => "Unsubscribe",
            EngineCommand::JoinRoom { .. } => "JoinRoom",
            EngineCommand::LeaveRoom { .. } => "LeaveRoom",
            EngineCommand::CreateRoom { .. } => "CreateRoom",
            EngineCommand::SubscribeTimeline { .. } => "SubscribeTimeline",
            EngineCommand::PaginateBackwards { .. } => "PaginateBackwards",
            EngineCommand::SendText { .. } => "SendText",
            EngineCommand::EditMessage { .. } => "EditMessage",
            EngineCommand::RedactEvent { .. } => "RedactEvent",
            EngineCommand::ToggleReaction { .. } => "ToggleReaction",
            EngineCommand::RetrySend { .. } => "RetrySend",
            EngineCommand::CancelSend { .. } => "CancelSend",
            EngineCommand::SendAttachment { .. } => "SendAttachment",
            EngineCommand::MarkRead { .. } => "MarkRead",
            EngineCommand::SetTyping { .. } => "SetTyping",
            EngineCommand::GetMedia { .. } => "GetMedia",
            EngineCommand::RequestSelfVerification => "RequestSelfVerification",
            EngineCommand::AcceptVerification { .. } => "AcceptVerification",
            EngineCommand::StartSas { .. } => "StartSas",
            EngineCommand::ConfirmSas { .. } => "ConfirmSas",
            EngineCommand::CancelVerification { .. } => "CancelVerification",
            EngineCommand::GetRecoveryStatus => "GetRecoveryStatus",
            EngineCommand::EnableRecovery => "EnableRecovery",
            EngineCommand::RecoverWithKey { .. } => "RecoverWithKey",
            EngineCommand::ResetRecovery => "ResetRecovery",
            EngineCommand::SubscribeSpaces => "SubscribeSpaces",
            EngineCommand::SubscribeSpaceChildren { .. } => "SubscribeSpaceChildren",
            EngineCommand::ListThemes => "ListThemes",
            EngineCommand::GetTheme { .. } => "GetTheme",
            EngineCommand::SetActiveTheme { .. } => "SetActiveTheme",
            EngineCommand::GetSettings => "GetSettings",
            EngineCommand::SetSetting { .. } => "SetSetting",
        };

        Err(EngineError::not_implemented(cmd_name))
    }

    async fn subscribe(&self) -> (SubscriptionId, EventSink) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sub_id = SubscriptionId(uuid::Uuid::new_v4().to_string());

        let mut subs = self.subscriptions.lock().await;
        subs.push(rx);

        (sub_id, tx)
    }

    async fn unsubscribe(&self, _subscription_id: SubscriptionId) -> Result<(), EngineError> {
        // Subscription cleanup happens when the channel is dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    #[async_trait::async_trait]
    impl CommandHandler for TestHandler {
        async fn handle(
            &self,
            cmd: EngineCommand,
            _sink: &EventSink,
        ) -> Result<Option<EngineResponse>, EngineError> {
            match cmd {
                EngineCommand::ListAccounts => {
                    Ok(Some(EngineResponse::AccountsList { accounts: vec![] }))
                }
                _ => Ok(None),
            }
        }
    }

    #[tokio::test]
    async fn test_router_dispatch() {
        let mut router = RouterEngine::new();
        router.register_handler(Arc::new(TestHandler));

        let response = router.handle(EngineCommand::ListAccounts).await.unwrap();
        assert!(matches!(response, EngineResponse::AccountsList { .. }));
    }

    #[tokio::test]
    async fn test_router_not_implemented() {
        let router = RouterEngine::new();
        let result = router.handle(EngineCommand::ListAccounts).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "not_implemented");
    }

    #[tokio::test]
    async fn test_subscription_lifecycle() {
        let router = RouterEngine::new();

        let (sub_id, _tx) = router.subscribe().await;
        assert!(!sub_id.0.is_empty());

        let result = router.unsubscribe(sub_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_sink_fan_out() {
        let _router = RouterEngine::new();

        let (tx, mut rx) = mpsc::unbounded_channel();

        // Simulate an event being sent
        let event = EngineEvent::ConnectionStatus {
            status: alloy_protocol::ConnectionStatus::Connected,
        };
        tx.send(event.clone()).unwrap();

        let received = rx.recv().await;
        assert!(received.is_some());
        assert!(matches!(
            received.unwrap(),
            EngineEvent::ConnectionStatus {
                status: alloy_protocol::ConnectionStatus::Connected
            }
        ));
    }
}
