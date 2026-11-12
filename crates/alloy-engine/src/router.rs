//! Command routing: one `GroupHandler` per functional area, dispatched by [`CommandGroup`].

use std::collections::HashMap;

use alloy_protocol::{EngineCommand, EngineResponse};
use alloy_store::AccountState as StoreAccountState;

use crate::accounts::start_stored_account;
use crate::context::EngineContext;
use crate::error::{EngineError, map_store};
use crate::sink::EventSink;

/// Functional command groups corresponding to owning implementation streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandGroup {
    Accounts,
    Rooms,
    Timeline,
    Media,
    Settings,
}

impl CommandGroup {
    /// Maps an `EngineCommand` variant to its responsible `CommandGroup`.
    pub fn from_command(cmd: &EngineCommand) -> Self {
        match cmd {
            EngineCommand::ListAccounts
            | EngineCommand::ListNetworks
            | EngineCommand::BeginLogin { .. }
            | EngineCommand::SubmitLogin { .. }
            | EngineCommand::CancelLogin { .. }
            | EngineCommand::Logout { .. } => CommandGroup::Accounts,

            EngineCommand::SubscribeRoomList { .. }
            | EngineCommand::Unsubscribe { .. }
            | EngineCommand::GetRoomMembers { .. }
            | EngineCommand::GetRoomDetails { .. }
            | EngineCommand::GetUserProfile { .. }
            | EngineCommand::GetRoomNote { .. }
            | EngineCommand::SetRoomNote { .. }
            | EngineCommand::GetTagCatalog
            | EngineCommand::CreateRoomTag { .. }
            | EngineCommand::RenameRoomTag { .. }
            | EngineCommand::ReorderRoomTags { .. }
            | EngineCommand::SetRoomTag { .. }
            | EngineCommand::DeleteRoomTag { .. }
            | EngineCommand::MarkRoomRead { .. }
            | EngineCommand::SetRoomPinned { .. }
            | EngineCommand::SetRoomArchived { .. }
            | EngineCommand::SetRoomMarkedUnread { .. }
            | EngineCommand::SetRoomMuted { .. }
            | EngineCommand::GetRoomMedia { .. }
            | EngineCommand::SearchRoomMessages { .. }
            | EngineCommand::SearchAllMessages { .. }
            | EngineCommand::ClearMediaCache
            | EngineCommand::RefreshSync
            | EngineCommand::SaveDraft { .. }
            | EngineCommand::GetDraft { .. }
            | EngineCommand::ListDrafts => CommandGroup::Rooms,

            EngineCommand::SubscribeTimeline { .. }
            | EngineCommand::CloseTimeline { .. }
            | EngineCommand::PaginateBackwards { .. }
            | EngineCommand::SendText { .. }
            | EngineCommand::EditMessage { .. }
            | EngineCommand::DeleteMessage { .. }
            | EngineCommand::ToggleReaction { .. }
            | EngineCommand::ForwardEvents { .. }
            | EngineCommand::RetrySend { .. }
            | EngineCommand::CancelSend { .. }
            | EngineCommand::MarkRead { .. }
            | EngineCommand::SetTyping { .. } => CommandGroup::Timeline,

            EngineCommand::GetMedia { .. }
            | EngineCommand::SendAttachment { .. }
            | EngineCommand::SendVoice { .. } => CommandGroup::Media,

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
        .and_then(|value| {
            value
                .get("action")
                .and_then(|action| action.as_str().map(String::from))
        })
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
        router.register_handler(Box::new(crate::handlers::accounts::AccountsHandler::new()));
        router.register_handler(Box::new(crate::handlers::rooms::RoomsHandler::new()));
        router.register_handler(Box::new(crate::handlers::timeline::TimelineHandler::new()));
        router.register_handler(Box::new(crate::handlers::media::MediaHandler::new()));
        router.register_handler(Box::new(crate::handlers::settings::SettingsHandler::new()));
        Self { router }
    }

    /// Starts one actor and one outbox worker per stored, active account.
    ///
    /// Errors are logged per account: one broken session must not prevent the others from
    /// syncing, and the account is marked for a fresh login instead.
    pub async fn start(&self) -> Result<(), EngineError> {
        let ctx = self.router.context();
        if !ctx.background {
            return Ok(());
        }
        // No worker is running yet, so recovering rows a previous run claimed cannot steal a live
        // send: the store-wide reset is only safe before the workers start.
        if let Err(err) = ctx.store.reset_in_flight_outbox().await {
            tracing::warn!(error = %err, "cannot recover interrupted outbox rows");
        }
        let records = ctx.store.list_accounts().await.map_err(map_store)?;
        for record in records {
            if record.state != StoreAccountState::Active {
                continue;
            }
            if let Err(err) = start_stored_account(ctx, record).await {
                tracing::warn!(error = %err.message, "account could not be started");
            }
        }
        Ok(())
    }

    /// Stops every account actor; the stored data stays untouched.
    pub async fn stop(&self) {
        self.router.context().accounts.stop_all().await;
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
