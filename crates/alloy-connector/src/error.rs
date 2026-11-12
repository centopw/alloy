use serde::{Deserialize, Serialize};

/// Everything a connector can fail with.
///
/// The error is part of the serializable contract: it crosses the sidecar's NDJSON channel and,
/// in M6, the gateway's WSS channel unchanged. Variants are internally tagged (`"type"`), so a
/// frame carries `{"type": "rate_limited", "retry_after_ms": 3000}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectorError {
    /// The stored session is no longer valid. The host must run the login flow again; retrying the
    /// call cannot succeed.
    #[error("unauthorized: re-login required")]
    Unauthorized,

    /// The network asked us to back off. Wait at least `retry_after_ms` before retrying.
    #[error("rate limited: retry after {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },

    /// The network cannot do this, or the connector does not implement it. `feature` names the
    /// capability (for example `"polls"`), so the host can explain what is missing.
    #[error("unsupported feature: {feature}")]
    NotSupported { feature: String },

    /// Transport failure (offline, TLS, DNS, a closed socket). Usually retryable.
    #[error("network error: {message}")]
    Network { message: String },

    /// The remote peer or the sidecar violated the connector protocol. Retrying the same call is
    /// pointless; the host should recreate the transport (restart the sidecar in M3).
    #[error("protocol error: {message}")]
    Protocol { message: String },

    /// The caller passed something the connector cannot accept (empty body and no attachment,
    /// unknown chat, malformed cursor, unwritable destination).
    #[error("invalid input: {message}")]
    InvalidInput { message: String },
}

impl ConnectorError {
    /// Whether retrying the same call, unchanged, can plausibly succeed later.
    ///
    /// `Unauthorized` is not retryable (the host must re-login), and `NotSupported`,
    /// `Protocol` and `InvalidInput` are caller or contract bugs. Everything retryable is either
    /// transport-level (`Network`) or a backoff request (`RateLimited`).
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Network { .. })
    }
}
