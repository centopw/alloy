use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};

/// Handler for encryption and verification operations (`CommandGroup::Crypto`).
///
/// Owned by stream: `core-crypto`
#[derive(Default)]
pub struct CryptoHandler;

impl CryptoHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for CryptoHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Crypto
    }

    async fn handle(
        &self,
        _ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::RequestSelfVerification
            | EngineCommand::AcceptVerification { .. }
            | EngineCommand::StartSas { .. }
            | EngineCommand::ConfirmSas { .. }
            | EngineCommand::CancelVerification { .. }
            | EngineCommand::GetRecoveryStatus
            | EngineCommand::EnableRecovery
            | EngineCommand::RecoverWithKey { .. }
            | EngineCommand::ResetRecovery => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Crypto group",
                name
            ))),
        }
    }
}
