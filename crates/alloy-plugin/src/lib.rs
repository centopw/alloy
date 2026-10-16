//! Alloy WASM plugin host
//!
//! Loads and executes WASM components (compiled to wasm32-wasip2) with strict
//! resource limits, capability-based security, and fuel metering.

use std::collections::HashMap;
use thiserror::Error;

pub mod host;
pub mod manifest;

pub use host::alloy::plugin::types as wit_types;
pub use host::exports::alloy::plugin::guest as wit_guest;
pub use host::{AlloyPlugin, HostState, PluginHost, PluginInstance};
pub use manifest::{PluginCapability, PluginManifest};

/// Result type for plugin operations
pub type PluginResult<T> = Result<T, PluginError>;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Failed to instantiate WASM component: {0}")]
    InstantiationError(String),

    #[error("Plugin trap/fuel exhaustion: {0}")]
    ExecutionError(String),

    #[error("Memory limit exceeded")]
    MemoryExceeded,

    #[error("Capability denied: {0}")]
    CapabilityDenied(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Limits for plugin execution
#[derive(Clone, Debug)]
pub struct PluginLimits {
    /// Maximum memory per instance in bytes (default 32 MB)
    pub max_memory_bytes: u64,
    /// Fuel units per synchronous call (default 100,000)
    pub sync_fuel_limit: u64,
    /// Fuel units per async call (default 10,000,000)
    pub async_fuel_limit: u64,
    /// Wall-clock timeout for async operations in ms (default 5000)
    pub async_timeout_ms: u64,
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 32 * 1024 * 1024, // 32 MB
            sync_fuel_limit: 100_000,
            async_fuel_limit: 10_000_000,
            async_timeout_ms: 5_000,
        }
    }
}

/// Plugin instance context: manifest, limits, and capability enforcement
pub struct PluginContext {
    pub manifest: PluginManifest,
    pub limits: PluginLimits,
    pub storage: HashMap<String, Vec<u8>>,
}

impl PluginContext {
    /// Create a new plugin context
    pub fn new(manifest: PluginManifest, limits: PluginLimits) -> Self {
        Self {
            manifest,
            limits,
            storage: HashMap::new(),
        }
    }

    /// Check if a capability is granted
    pub fn has_capability(&self, cap: &str) -> bool {
        self.manifest.permissions.iter().any(|p| p.name == cap)
    }

    /// Get KV storage quota for this plugin
    pub fn kv_quota_bytes(&self) -> u64 {
        self.manifest
            .permissions
            .iter()
            .find(|p| p.name == "storage:kv")
            .and_then(|p| p.quota_kb)
            .unwrap_or(5 * 1024) as u64
            * 1024
    }

    /// Get network hosts allowlist
    pub fn network_allowlist(&self) -> Vec<String> {
        self.manifest
            .permissions
            .iter()
            .find(|p| p.name == "network:fetch")
            .and_then(|p| p.hosts.clone())
            .unwrap_or_default()
    }
}

/// Declare WIT bindings generated from wit/ directory
mod generated {
    // WIT bindings are generated at compile time via build.rs
    // We'll use a simple test-friendly approach for now.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = PluginLimits::default();
        assert_eq!(limits.max_memory_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.sync_fuel_limit, 100_000);
    }
}
