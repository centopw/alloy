use crate::{PluginContext, PluginError, PluginLimits, PluginManifest, PluginResult};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::Component;
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};

wasmtime::component::bindgen!({
    path: "wit/alloy-plugin.wit",
    world: "alloy-plugin",
});
pub struct PluginHost {
    engine: Engine,
    limits: PluginLimits,
}

impl PluginHost {
    /// Create a new plugin host with default limits and Cranelift JIT
    pub fn new(limits: PluginLimits) -> PluginResult<Self> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine =
            Engine::new(&config).map_err(|e| PluginError::InstantiationError(e.to_string()))?;
        Ok(Self { engine, limits })
    }

    /// Create a plugin host with Pulley interpreter (for iOS/no-JIT environments)
    /// Falls back to Cranelift if Pulley is not available
    pub fn new_with_pulley(limits: PluginLimits) -> PluginResult<Self> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let _result = config.target("pulley64");
        let engine =
            Engine::new(&config).map_err(|e| PluginError::InstantiationError(e.to_string()))?;
        Ok(Self { engine, limits })
    }

    /// Load a component from a WASM file and get a plugin instance
    pub fn load_component<P: AsRef<Path>>(
        &self,
        wasm_path: P,
        manifest: PluginManifest,
    ) -> PluginResult<PluginInstance> {
        let wasm_bytes = fs::read(wasm_path)?;
        self.load_component_bytes(&wasm_bytes, manifest)
    }

    /// Load a component from raw WASM bytes
    pub fn load_component_bytes(
        &self,
        wasm_bytes: &[u8],
        manifest: PluginManifest,
    ) -> PluginResult<PluginInstance> {
        // Load the component
        let component = Component::new(&self.engine, wasm_bytes).map_err(|e| {
            PluginError::InstantiationError(format!("Failed to load component: {}", e))
        })?;

        let context = Arc::new(PluginContext::new(manifest, self.limits.clone()));

        Ok(PluginInstance {
            component,
            context,
            engine: self.engine.clone(),
        })
    }
}

/// A loaded plugin instance with its context
pub struct PluginInstance {
    component: Component,
    context: Arc<PluginContext>,
    engine: Engine,
}

impl PluginInstance {
    /// Instantiate the component in a new store with limits applied
    pub fn create_store(&self) -> PluginResult<Store<HostState>> {
        // Create a store with memory limits
        let _limits = StoreLimitsBuilder::new()
            .memory_size(self.context.limits.max_memory_bytes as usize)
            .build();

        let state = HostState::new(self.context.clone());
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.store_limiter);
        store
            .set_fuel(self.context.limits.sync_fuel_limit)
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        store.set_epoch_deadline(1);
        Ok(store)
    }

    /// Get the component
    pub fn component(&self) -> &Component {
        &self.component
    }

    /// Get the context
    pub fn context(&self) -> &PluginContext {
        &self.context
    }
}

use std::collections::HashMap;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// Host state passed into the WASM store
pub struct HostState {
    pub context: Arc<PluginContext>,
    pub store_limiter: StoreLimits,
    pub logged_messages: Vec<String>,
    pub kv_storage: HashMap<String, Vec<u8>>,
    pub sent_messages: Vec<(String, String)>,
    pub wasi_ctx: WasiCtx,
    pub resource_table: ResourceTable,
}

impl HostState {
    pub fn new(context: Arc<PluginContext>) -> Self {
        let store_limiter = StoreLimitsBuilder::new()
            .memory_size(context.limits.max_memory_bytes as usize)
            .build();
        let wasi_ctx = WasiCtxBuilder::new().build();
        let resource_table = ResourceTable::new();
        Self {
            context,
            store_limiter,
            logged_messages: Vec::new(),
            kv_storage: HashMap::new(),
            sent_messages: Vec::new(),
            wasi_ctx,
            resource_table,
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

impl alloy::plugin::types::Host for HostState {}

impl alloy::plugin::host::Host for HostState {
    fn log(&mut self, level: alloy::plugin::types::LogLevel, message: String) {
        self.logged_messages
            .push(format!("{:?}: {}", level, message));
    }

    fn kv_get(&mut self, key: String) -> Result<Option<Vec<u8>>, String> {
        if !self.context.has_capability("storage:kv") {
            return Err("Permission denied: storage:kv capability required".to_string());
        }
        Ok(self.kv_storage.get(&key).cloned())
    }

    fn kv_set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        if !self.context.has_capability("storage:kv") {
            return Err("Permission denied: storage:kv capability required".to_string());
        }
        let quota = self.context.kv_quota_bytes();
        let current_size: usize = self.kv_storage.values().map(|v| v.len()).sum();
        if (current_size + value.len()) as u64 > quota {
            return Err(format!("Quota exceeded: {} bytes limit", quota));
        }
        self.kv_storage.insert(key, value);
        Ok(())
    }

    fn send_message(&mut self, room_id: String, body: String) -> Result<String, String> {
        if !self.context.has_capability("room:send") {
            return Err("Permission denied: room:send capability required".to_string());
        }
        let event_id = format!("$msg_{}", self.sent_messages.len() + 1);
        self.sent_messages.push((room_id, body));
        Ok(event_id)
    }
}

impl PluginInstance {
    /// Instantiate the plugin component and prepare it for function calls
    pub fn instantiate(&self) -> PluginResult<(AlloyPlugin, Store<HostState>)> {
        let mut linker = wasmtime::component::Linker::<HostState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| PluginError::InstantiationError(format!("Failed to add WASI: {}", e)))?;
        AlloyPlugin::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |s: &mut HostState| s,
        )
        .map_err(|e| PluginError::InstantiationError(e.to_string()))?;

        let mut store = self.create_store()?;
        let plugin =
            AlloyPlugin::instantiate(&mut store, &self.component, &linker).map_err(|e| {
                PluginError::InstantiationError(format!("Failed to instantiate component: {}", e))
            })?;

        Ok((plugin, store))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_host_creation() {
        let limits = PluginLimits::default();
        let _host = PluginHost::new(limits).unwrap();
        // Just verify creation succeeds
    }
}
