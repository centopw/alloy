use super::rooms::map_err;
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
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::GetSpaceParents => {
                let client = ctx.active_client().await?;
                let parents = client.space_parents().await.map_err(map_err)?;
                Ok(EngineResponse::SpaceParents { parents })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Spaces group",
                name
            ))),
        }
    }
}
