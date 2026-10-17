use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for timeline/messaging operations (`CommandGroup::Timeline`).
///
/// Owned by stream: `core-rooms-timeline`
#[derive(Default)]
pub struct TimelineHandler;

impl TimelineHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for TimelineHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Timeline
    }

    async fn handle(
        &self,
        _ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::SubscribeTimeline { .. }
            | EngineCommand::PaginateBackwards { .. }
            | EngineCommand::SendText { .. }
            | EngineCommand::EditMessage { .. }
            | EngineCommand::RedactEvent { .. }
            | EngineCommand::ToggleReaction { .. }
            | EngineCommand::RetrySend { .. }
            | EngineCommand::CancelSend { .. }
            | EngineCommand::SendAttachment { .. }
            | EngineCommand::MarkRead { .. }
            | EngineCommand::SetTyping { .. } => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Timeline group",
                name
            ))),
        }
    }
}
