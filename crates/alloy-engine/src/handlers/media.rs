use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for media operations (`CommandGroup::Media`).
///
/// Owned by stream: `core-media`
#[derive(Default)]
pub struct MediaHandler;

impl MediaHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for MediaHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Media
    }

    async fn handle(
        &self,
        _ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::GetMedia { .. } => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Media group",
                name
            ))),
        }
    }
}
