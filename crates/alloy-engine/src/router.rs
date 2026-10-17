use crate::context::EngineContext;
use crate::error::EngineError;
use crate::sink::EventSink;
use alloy_protocol::{EngineCommand, EngineResponse};
use std::collections::HashMap;

/// Functional command groups corresponding to owning implementation streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandGroup {
    Auth,
    Rooms,
    Timeline,
    Crypto,
    Media,
    Spaces,
    Settings,
}

impl CommandGroup {
    /// Maps an `EngineCommand` variant to its responsible `CommandGroup`.
    pub fn from_command(cmd: &EngineCommand) -> Self {
        match cmd {
            EngineCommand::DiscoverHomeserver { .. }
            | EngineCommand::LoginPassword { .. }
            | EngineCommand::StartOidcLogin { .. }
            | EngineCommand::CompleteOidcLogin { .. }
            | EngineCommand::RestoreSession { .. }
            | EngineCommand::Logout { .. }
            | EngineCommand::ListAccounts => CommandGroup::Auth,

            EngineCommand::SubscribeRoomList { .. }
            | EngineCommand::Unsubscribe { .. }
            | EngineCommand::JoinRoom { .. }
            | EngineCommand::LeaveRoom { .. }
            | EngineCommand::CreateRoom { .. }
            | EngineCommand::GetRoomMembers { .. }
            | EngineCommand::GetRoomDetails { .. } => CommandGroup::Rooms,

            EngineCommand::SubscribeTimeline { .. }
            | EngineCommand::PaginateBackwards { .. }
            | EngineCommand::SendText { .. }
            | EngineCommand::EditMessage { .. }
            | EngineCommand::RedactEvent { .. }
            | EngineCommand::ToggleReaction { .. }
            | EngineCommand::RetrySend { .. }
            | EngineCommand::CancelSend { .. }
            | EngineCommand::MarkRead { .. }
            | EngineCommand::SetTyping { .. } => CommandGroup::Timeline,

            EngineCommand::RequestSelfVerification
            | EngineCommand::AcceptVerification { .. }
            | EngineCommand::StartSas { .. }
            | EngineCommand::ConfirmSas { .. }
            | EngineCommand::CancelVerification { .. }
            | EngineCommand::GetRecoveryStatus
            | EngineCommand::EnableRecovery { .. }
            | EngineCommand::RecoverWithKey { .. }
            | EngineCommand::ResetRecovery { .. } => CommandGroup::Crypto,

            EngineCommand::GetMedia { .. } | EngineCommand::SendAttachment { .. } => {
                CommandGroup::Media
            }

            EngineCommand::GetSpaceParents => CommandGroup::Spaces,

            EngineCommand::ListThemes
            | EngineCommand::GetTheme { .. }
            | EngineCommand::SetActiveTheme { .. }
            | EngineCommand::GetSettings
            | EngineCommand::SetSetting { .. } => CommandGroup::Settings,
        }
    }
}

/// Derives the command action name dynamically using serde serialization.
pub fn command_name(cmd: &EngineCommand) -> String {
    serde_json::to_value(cmd)
        .ok()
        .and_then(|v| v.get("action").and_then(|a| a.as_str().map(String::from)))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Trait implemented by domain group handlers.
#[async_trait::async_trait]
pub trait GroupHandler: Send + Sync {
    fn group(&self) -> CommandGroup;
    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError>;
}

/// Core engine contract.
#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    async fn handle(&self, cmd: EngineCommand) -> Result<EngineResponse, EngineError>;
    fn context(&self) -> &EngineContext;
    fn sink(&self) -> &EventSink;
}

/// Dispatches commands to registered `GroupHandler` instances by `CommandGroup`.
pub struct RouterEngine {
    context: EngineContext,
    handlers: HashMap<CommandGroup, Box<dyn GroupHandler>>,
}

impl RouterEngine {
    pub fn new(context: EngineContext) -> Self {
        Self {
            context,
            handlers: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, handler: Box<dyn GroupHandler>) {
        self.handlers.insert(handler.group(), handler);
    }
}

#[async_trait::async_trait]
impl Engine for RouterEngine {
    async fn handle(&self, cmd: EngineCommand) -> Result<EngineResponse, EngineError> {
        let group = CommandGroup::from_command(&cmd);
        if let Some(handler) = self.handlers.get(&group) {
            handler.handle(&self.context, cmd).await
        } else {
            let name = command_name(&cmd);
            Err(EngineError::not_implemented(&name))
        }
    }

    fn context(&self) -> &EngineContext {
        &self.context
    }

    fn sink(&self) -> &EventSink {
        &self.context.sink
    }
}

/// Concrete application engine pre-wired with all default group handlers.
pub struct AppEngine {
    router: RouterEngine,
}

impl AppEngine {
    pub fn new(ctx: EngineContext) -> Self {
        let mut router = RouterEngine::new(ctx);
        router.register_handler(Box::new(crate::handlers::auth::AuthHandler::new()));
        router.register_handler(Box::new(crate::handlers::rooms::RoomsHandler::new()));
        router.register_handler(Box::new(crate::handlers::timeline::TimelineHandler::new()));
        router.register_handler(Box::new(crate::handlers::crypto::CryptoHandler::new()));
        router.register_handler(Box::new(crate::handlers::media::MediaHandler::new()));
        router.register_handler(Box::new(crate::handlers::spaces::SpacesHandler::new()));
        router.register_handler(Box::new(crate::handlers::settings::SettingsHandler::new()));
        Self { router }
    }

    pub fn router(&self) -> &RouterEngine {
        &self.router
    }

    pub fn router_mut(&mut self) -> &mut RouterEngine {
        &mut self.router
    }
}

#[async_trait::async_trait]
impl Engine for AppEngine {
    async fn handle(&self, cmd: EngineCommand) -> Result<EngineResponse, EngineError> {
        self.router.handle(cmd).await
    }

    fn context(&self) -> &EngineContext {
        self.router.context()
    }

    fn sink(&self) -> &EventSink {
        self.router.sink()
    }
}
