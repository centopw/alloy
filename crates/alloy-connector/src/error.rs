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

    /// The account has no PIN-protected encrypted archive to read. This is a capability, not a
    /// failure: the account simply never set one up, and nothing logs it as a warning.
    #[error("this account has no encrypted history archive")]
    PinBackupUnavailable,

    /// The archive rejected the PIN. `attempts_remaining` is how many tries are left before the
    /// archive locks itself.
    #[error("the archive PIN was rejected: {attempts_remaining} attempts remain")]
    PinRejected {
        /// How many tries are left. The sidecar omits it when the network does not say.
        #[serde(default)]
        attempts_remaining: u32,
    },

    /// Too many rejected PINs; the archive refuses further attempts until it is unlocked on the
    /// network's own site or app.
    #[error("the archive PIN is locked")]
    PinLockedOut,
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
