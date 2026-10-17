use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for authentication commands (`CommandGroup::Auth`).
///
/// Owned by stream: `core-auth`
#[derive(Default)]
pub struct AuthHandler;

impl AuthHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for AuthHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Auth
    }

    async fn handle(
        &self,
        _ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::DiscoverHomeserver { .. }
            | EngineCommand::LoginPassword { .. }
            | EngineCommand::StartOidcLogin { .. }
            | EngineCommand::CompleteOidcLogin { .. }
            | EngineCommand::RestoreSession { .. }
            | EngineCommand::Logout { .. }
            | EngineCommand::ListAccounts => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Auth group",
                name
            ))),
        }
    }
}
