use alloy_plugin::{PluginContext, PluginHost, PluginLimits, PluginManifest};
use std::fs;
use std::path::PathBuf;

/// Helper to load the test manifest
fn load_test_manifest() -> PluginManifest {
    PluginManifest {
        id: "test.hello-plugin".to_string(),
        name: Some("Hello Plugin".to_string()),
        version: "0.1.0".to_string(),
        api_version: "0.1.0".to_string(),
        entrypoint: "hello_plugin.wasm".to_string(),
        description: Some("Test hello plugin".to_string()),
        permissions: vec![
            alloy_plugin::PluginCapability {
                name: "log".to_string(),
                scope: None,
                reason: Some("Logging".to_string()),
                hosts: None,
                quota_kb: None,
                slots: None,
            },
            alloy_plugin::PluginCapability {
                name: "storage:kv".to_string(),
                scope: None,
                reason: Some("Key-value storage".to_string()),
                hosts: None,
                quota_kb: Some(5120),
                slots: None,
            },
            alloy_plugin::PluginCapability {
                name: "room:send".to_string(),
                scope: Some("active".to_string()),
                reason: Some("Send messages".to_string()),
                hosts: None,
                quota_kb: None,
                slots: None,
            },
        ],
        contributes: None,
    }
}

static HELLO_PLUGIN_WASM: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
    // 1. Allow override via ALLOY_HELLO_WASM env var
    if let Ok(path_str) = std::env::var("ALLOY_HELLO_WASM") {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return p;
        }
        panic!(
            "ALLOY_HELLO_WASM specified but file does not exist: {}",
            p.display()
        );
    }

    // 2. Locate guest manifest
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to locate workspace root");
    let guest_manifest = workspace_root.join("packages/hello-plugin/Cargo.toml");
    assert!(
        guest_manifest.exists(),
        "Guest manifest not found at {}",
        guest_manifest.display()
    );

    // 3. Build under test's target dir
    let target_dir = option_env!("CARGO_TARGET_TMPDIR")
        .map(|s| PathBuf::from(s).join("hello-plugin-target"))
        .unwrap_or_else(|| std::env::temp_dir().join("alloy-hello-plugin-target"));

    let wasm_path = target_dir.join("wasm32-wasip2/debug/hello_plugin.wasm");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&guest_manifest)
        .arg("--target")
        .arg("wasm32-wasip2")
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("Failed to execute cargo build for hello-plugin");

    if !status.success() {
        let target_check = std::process::Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output();
        let has_target = target_check
            .map(|out| String::from_utf8_lossy(&out.stdout).contains("wasm32-wasip2"))
            .unwrap_or(false);

        if !has_target {
            panic!(
                "Failed to build hello-plugin because target wasm32-wasip2 is missing.\n\
                         Please install it by running:\n    rustup target add wasm32-wasip2"
            );
        } else {
            panic!(
                "Failed to build hello-plugin guest component (cargo build exited with status {})",
                status
            );
        }
    }

    assert!(
        wasm_path.exists(),
        "hello_plugin.wasm was not found at expected location: {}",
        wasm_path.display()
    );

    wasm_path
});

/// Helper to get the path to the compiled hello-plugin, building it hermetically on demand.
fn get_hello_plugin_path() -> PathBuf {
    HELLO_PLUGIN_WASM.clone()
}

#[test]
fn test_plugin_host_creation() {
    let limits = PluginLimits::default();
    let _host = PluginHost::new(limits).expect("Failed to create plugin host");
}

#[test]
fn test_plugin_host_with_pulley() {
    let limits = PluginLimits::default();
    let _host =
        PluginHost::new_with_pulley(limits).expect("Failed to create plugin host with pulley");
}

#[test]
fn test_plugin_context_capabilities() {
    let manifest = load_test_manifest();
    let context = PluginContext::new(manifest, PluginLimits::default());

    assert!(context.has_capability("storage:kv"));
    assert!(context.has_capability("room:send"));
    assert!(!context.has_capability("network:fetch"));
    assert!(!context.has_capability("room:read"));
}

#[test]
fn test_plugin_context_kv_quota() {
    let manifest = load_test_manifest();
    let context = PluginContext::new(manifest, PluginLimits::default());

    // Manifest specifies 5120 KB for storage:kv
    assert_eq!(context.kv_quota_bytes(), 5120 * 1024);
}

#[test]
fn test_plugin_context_default_kv_quota() {
    let manifest = PluginManifest {
        id: "test.no-kv".to_string(),
        name: None,
        version: "0.1.0".to_string(),
        api_version: "0.1.0".to_string(),
        entrypoint: "test.wasm".to_string(),
        description: None,
        permissions: vec![], // No permissions including KV
        contributes: None,
    };
    let context = PluginContext::new(manifest, PluginLimits::default());

    // Should default to 5 MB
    assert_eq!(context.kv_quota_bytes(), 5 * 1024 * 1024);
}

#[test]
fn test_load_hello_plugin() {
    let hello_path = get_hello_plugin_path();

    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("Failed to create host");
    let manifest = load_test_manifest();

    let _instance = host
        .load_component(&hello_path, manifest)
        .expect("Failed to load hello-plugin");
}

#[test]
fn test_load_plugin_from_bytes() {
    let hello_path = get_hello_plugin_path();

    let wasm_bytes = fs::read(&hello_path).expect("Failed to read hello-plugin WASM");
    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("Failed to create host");
    let manifest = load_test_manifest();

    let _instance = host
        .load_component_bytes(&wasm_bytes, manifest)
        .expect("Failed to load plugin from bytes");
}

#[test]
fn test_plugin_instance_store_creation() {
    let hello_path = get_hello_plugin_path();

    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("Failed to create host");
    let manifest = load_test_manifest();

    let instance = host
        .load_component(&hello_path, manifest)
        .expect("Failed to load plugin");

    // Verify we can create a store with limits
    let _store = instance.create_store().expect("Failed to create store");
}

#[test]
fn test_hello_plugin_full_lifecycle() {
    let hello_path = get_hello_plugin_path();

    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("Failed to create host");
    let manifest = load_test_manifest();

    let instance = host
        .load_component(&hello_path, manifest)
        .expect("Failed to load plugin");

    let (plugin, mut store) = instance.instantiate().expect("Failed to instantiate");

    // 1. Call init()
    let init_res = plugin
        .alloy_plugin_guest()
        .call_init(&mut store)
        .expect("call_init invocation failed");
    assert!(init_res.is_ok(), "init() returned error: {:?}", init_res);

    // Check host logged messages and KV storage
    assert!(
        store
            .data()
            .logged_messages
            .iter()
            .any(|m| m.contains("initialized"))
    );
    assert_eq!(
        store.data().kv_storage.get("init_marker"),
        Some(&b"initialized"[..].to_vec())
    );

    // 2. Call on_event()
    let event = alloy_plugin::wit_types::RoomEvent {
        event_id: "$evt123".to_string(),
        room_id: "!room:example.org".to_string(),
        sender: "@alice:example.org".to_string(),
        body: "Hello from Alice!".to_string(),
        timestamp: 1700000000,
    };
    let event_res = plugin
        .alloy_plugin_guest()
        .call_on_event(&mut store, &event)
        .expect("call_on_event invocation failed");
    assert!(
        event_res.is_ok(),
        "on_event() returned error: {:?}",
        event_res
    );

    // Check event count in KV storage
    assert_eq!(
        store.data().kv_storage.get("event_count_!room:example.org"),
        Some(&b"1"[..].to_vec())
    );

    // 3. Call render_panel()
    let panel_res = plugin
        .alloy_plugin_guest()
        .call_render_panel(&mut store)
        .expect("call_render_panel invocation failed");
    assert!(
        panel_res.is_ok(),
        "render_panel() returned error: {:?}",
        panel_res
    );
    let panel_json = panel_res.unwrap();
    assert!(panel_json.contains("Hello Plugin"));
    let parsed: serde_json::Value =
        serde_json::from_str(&panel_json).expect("valid JSON view string");
    assert_eq!(parsed["type"], "panel");
    assert_eq!(parsed["title"], "Hello Plugin");
}

#[test]
fn test_infinite_loop_stopped_by_fuel() {
    let hello_path = get_hello_plugin_path();
    let limits = PluginLimits {
        sync_fuel_limit: 500_000, // Enough for instantiation
        ..Default::default()
    };
    let host = PluginHost::new(limits).expect("create host");
    let manifest = load_test_manifest();
    let instance = host.load_component(&hello_path, manifest).expect("load");
    let (plugin, mut store) = instance.instantiate().expect("instantiate");

    // Set a small fuel limit for the call so the infinite loop quickly exhausts it
    store.set_fuel(5_000).expect("set fuel");
    let event = alloy_plugin::wit_types::RoomEvent {
        event_id: "$loop1".to_string(),
        room_id: "!test:example.org".to_string(),
        sender: "@alice:example.org".to_string(),
        body: "loop".to_string(), // Triggers infinite loop
        timestamp: 100,
    };

    let res = plugin
        .alloy_plugin_guest()
        .call_on_event(&mut store, &event);
    assert!(
        res.is_err(),
        "Expected fuel exhaustion trap, but call succeeded"
    );
    let err = res.unwrap_err();
    assert!(
        err.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)
            || format!("{:?}", err).contains("OutOfFuel")
            || format!("{}", err).contains("wasm backtrace"),
        "Expected OutOfFuel or trap, got: {:?}",
        err
    );
}

#[test]
fn test_memory_hog_stopped_by_cap() {
    let hello_path = get_hello_plugin_path();
    // Enforce strict 16 MiB memory cap
    let limits = PluginLimits {
        max_memory_bytes: 16 * 1024 * 1024,
        sync_fuel_limit: 10_000_000,
        ..Default::default()
    };
    let host = PluginHost::new(limits).expect("create host");
    let manifest = load_test_manifest();
    let instance = host.load_component(&hello_path, manifest).expect("load");
    let (plugin, mut store) = instance.instantiate().expect("instantiate");

    let event = alloy_plugin::wit_types::RoomEvent {
        event_id: "$hog1".to_string(),
        room_id: "!test:example.org".to_string(),
        sender: "@alice:example.org".to_string(),
        body: "allocate_hog".to_string(), // Allocates 40 MiB
        timestamp: 100,
    };

    let res = plugin
        .alloy_plugin_guest()
        .call_on_event(&mut store, &event);
    // In WebAssembly, failing to grow memory triggers an out of memory panic or trap
    assert!(
        res.is_err() || res.as_ref().unwrap().is_err(),
        "Expected memory cap violation"
    );
}

#[test]
fn test_send_message_capability_granted() {
    let hello_path = get_hello_plugin_path();
    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("create host");
    let manifest = load_test_manifest(); // Contains room:send capability
    let instance = host.load_component(&hello_path, manifest).expect("load");
    let (plugin, mut store) = instance.instantiate().expect("instantiate");

    let event = alloy_plugin::wit_types::RoomEvent {
        event_id: "$ping1".to_string(),
        room_id: "!room:example.org".to_string(),
        sender: "@alice:example.org".to_string(),
        body: "ping".to_string(), // Calls host::send_message
        timestamp: 100,
    };

    let res = plugin
        .alloy_plugin_guest()
        .call_on_event(&mut store, &event)
        .expect("call succeeded");
    assert!(
        res.is_ok(),
        "Expected send_message to succeed, got: {:?}",
        res
    );
    assert_eq!(store.data().sent_messages.len(), 1);
    assert_eq!(
        store.data().sent_messages[0],
        ("!room:example.org".to_string(), "pong".to_string())
    );
}

#[test]
fn test_send_message_capability_denied() {
    let hello_path = get_hello_plugin_path();
    let limits = PluginLimits::default();
    let host = PluginHost::new(limits).expect("create host");

    // Manifest without room:send capability
    let mut manifest = load_test_manifest();
    manifest.permissions.retain(|p| p.name != "room:send");

    let instance = host.load_component(&hello_path, manifest).expect("load");
    let (plugin, mut store) = instance.instantiate().expect("instantiate");

    let event = alloy_plugin::wit_types::RoomEvent {
        event_id: "$ping2".to_string(),
        room_id: "!room:example.org".to_string(),
        sender: "@alice:example.org".to_string(),
        body: "ping".to_string(), // Calls host::send_message
        timestamp: 100,
    };

    let res = plugin
        .alloy_plugin_guest()
        .call_on_event(&mut store, &event)
        .expect("call completed");
    // Guest maps host error into Result::Err(String)
    assert!(res.is_err(), "Expected send_message to be denied");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("Permission denied: room:send"),
        "Unexpected error: {}",
        err_msg
    );
    assert_eq!(store.data().sent_messages.len(), 0);
}

#[test]
fn test_capability_denial() {
    // Create a manifest that requests a capability
    let manifest = PluginManifest {
        id: "test.denied-cap".to_string(),
        name: None,
        version: "0.1.0".to_string(),
        api_version: "0.1.0".to_string(),
        entrypoint: "test.wasm".to_string(),
        description: None,
        permissions: vec![], // No capabilities granted
        contributes: None,
    };

    let context = PluginContext::new(manifest, PluginLimits::default());

    // Verify that capability checks fail
    assert!(!context.has_capability("room:send"));
    assert!(!context.has_capability("network:fetch"));
    assert!(!context.has_capability("storage:kv"));
}

#[test]
fn test_network_allowlist_extraction() {
    let manifest = PluginManifest {
        id: "test.network".to_string(),
        name: None,
        version: "0.1.0".to_string(),
        api_version: "0.1.0".to_string(),
        entrypoint: "test.wasm".to_string(),
        description: None,
        permissions: vec![alloy_plugin::PluginCapability {
            name: "network:fetch".to_string(),
            scope: None,
            reason: None,
            hosts: Some(vec![
                "api.github.com".to_string(),
                "api.matrix.org".to_string(),
            ]),
            quota_kb: None,
            slots: None,
        }],
        contributes: None,
    };

    let context = PluginContext::new(manifest, PluginLimits::default());
    let allowlist = context.network_allowlist();

    assert_eq!(allowlist.len(), 2);
    assert!(allowlist.contains(&"api.github.com".to_string()));
    assert!(allowlist.contains(&"api.matrix.org".to_string()));
}

#[test]
fn test_network_allowlist_empty() {
    let manifest = PluginManifest {
        id: "test.no-network".to_string(),
        name: None,
        version: "0.1.0".to_string(),
        api_version: "0.1.0".to_string(),
        entrypoint: "test.wasm".to_string(),
        description: None,
        permissions: vec![], // No network capability
        contributes: None,
    };

    let context = PluginContext::new(manifest, PluginLimits::default());
    let allowlist = context.network_allowlist();

    assert!(allowlist.is_empty());
}

#[test]
fn test_manifest_validation_against_schema() {
    // Load the JSON schema
    let schema_path = "schemas/plugin-manifest.schema.json";
    if !std::path::Path::new(schema_path).exists() {
        eprintln!("Skipping schema validation: {} not found", schema_path);
        return;
    }

    let schema_json = fs::read_to_string(schema_path).expect("Failed to read schema");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_json).expect("Failed to parse schema");

    let manifest = load_test_manifest();
    let manifest_json = serde_json::to_value(&manifest).expect("Failed to serialize manifest");
    let validator = jsonschema::validator_for(&schema).expect("Failed to compile schema");

    assert!(validator.is_valid(&manifest_json));
}

#[test]
fn test_manifest_invalid_missing_required_field() {
    // Create an invalid manifest (missing entrypoint)
    let invalid_json = serde_json::json!({
        "id": "test.invalid",
        "version": "0.1.0",
        "api_version": "0.1.0",
        // Missing "entrypoint"
        "permissions": []
    });

    let schema_path = "schemas/plugin-manifest.schema.json";
    if !std::path::Path::new(schema_path).exists() {
        eprintln!("Skipping validation test: {} not found", schema_path);
        return;
    }

    let schema_json = fs::read_to_string(schema_path).expect("Failed to read schema");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_json).expect("Failed to parse schema");
    let validator = jsonschema::validator_for(&schema).expect("Failed to compile schema");

    assert!(!validator.is_valid(&invalid_json));
}

#[test]
fn test_limits_memory_cap() {
    let limits = PluginLimits {
        max_memory_bytes: 10 * 1024 * 1024, // 10 MB
        sync_fuel_limit: 100_000,
        async_fuel_limit: 10_000_000,
        async_timeout_ms: 5_000,
    };

    assert_eq!(limits.max_memory_bytes, 10 * 1024 * 1024);
}

#[test]
fn test_limits_fuel_budgets() {
    let limits = PluginLimits::default();

    assert_eq!(limits.sync_fuel_limit, 100_000);
    assert_eq!(limits.async_fuel_limit, 10_000_000);
    assert_eq!(limits.async_timeout_ms, 5_000);
}
