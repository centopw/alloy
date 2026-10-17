use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for room operations and subscriptions (`CommandGroup::Rooms`).
///
/// Owned by stream: `core-rooms-timeline`
#[derive(Default)]
pub struct RoomsHandler;

impl RoomsHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for RoomsHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Rooms
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::Unsubscribe { subscription_id } => {
                ctx.subscriptions.cancel(&subscription_id).await;
                Ok(EngineResponse::Unsubscribed { subscription_id })
            }
            EngineCommand::SubscribeRoomList { .. }
            | EngineCommand::JoinRoom { .. }
            | EngineCommand::LeaveRoom { .. }
            | EngineCommand::CreateRoom { .. } => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Rooms group",
                name
            ))),
        }
    }
}
