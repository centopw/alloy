use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for spaces operations (`CommandGroup::Spaces`).
///
/// Owned by stream: `core-spaces`
#[derive(Default)]
pub struct SpacesHandler;

impl SpacesHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for SpacesHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Spaces
    }

    async fn handle(
        &self,
        _ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::SubscribeSpaces | EngineCommand::SubscribeSpaceChildren { .. } => {
                Err(EngineError::not_implemented(&name))
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Spaces group",
                name
            ))),
        }
    }
}
