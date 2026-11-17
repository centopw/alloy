//! The engine's single error surface.
//!
//! Commands return the protocol's [`EngineError`], because that is what crosses the IPC boundary.
//! Everything below it is mapped here, once per error family, so a new call site cannot invent a
//! code that the UI has never seen.

use alloy_connector::ConnectorError;
use alloy_store::StoreError;

pub use alloy_protocol::EngineError;

/// A store failure is always a local defect: the database is on this device and the engine owns
/// every schema interaction.
pub fn map_store(err: StoreError) -> EngineError {
    EngineError::internal_error(format!("store: {err}"))
}

/// Maps a connector failure onto the stable codes the UI branches on.
///
/// - `Unauthorized` means the stored session is dead, so retrying is pointless.
/// - `RateLimited` keeps `network_error` but names `rate_limited` as the reason.
/// - `NotSupported` names the missing capability, so the UI can explain what the network lacks.
/// - `Protocol` is a contract violation, reported as an internal error rather than a retry.
pub fn map_connector(err: ConnectorError) -> EngineError {
    match err {
        ConnectorError::Unauthorized => {
            EngineError::auth_failed("the stored session is no longer valid")
        }
        ConnectorError::RateLimited { retry_after_ms } => EngineError {
            code: "network_error".to_string(),
            message: format!("the network asked us to slow down for {retry_after_ms} ms"),
            reason: Some("rate_limited".to_string()),
        },
        ConnectorError::NotSupported { feature } => EngineError::not_supported(feature),
        ConnectorError::Network { message } => EngineError::network_error(message),
        ConnectorError::Protocol { message } => {
            EngineError::internal_error(format!("connector protocol error: {message}"))
        }
        ConnectorError::InvalidInput { message } => EngineError::invalid_argument(message),
        // The archive's expected failures have their own command outcome; reaching here means a
        // call other than `EnrolHistory` saw one, which is a contract violation rather than a
        // capability the UI should offer.
        ConnectorError::PinBackupUnavailable => EngineError::not_supported("history_pin_backup"),
        ConnectorError::PinRejected { attempts_remaining } => EngineError::auth_failed(format!(
            "the archive PIN was rejected ({attempts_remaining} attempts remain)"
        )),
        ConnectorError::PinLockedOut => EngineError::auth_failed("the archive PIN is locked"),
    }
}
