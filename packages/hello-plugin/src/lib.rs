// HelloPlugin: A simple test guest plugin for the Alloy WASM plugin host
//
// This plugin demonstrates the three core hooks (init, on-event, render-panel)
// and uses logging and KV storage.

wit_bindgen::generate!();

use alloy::plugin::host::{self, LogLevel};
use exports::alloy::plugin::guest::{Guest, RoomEvent};

struct Component;

impl Guest for Component {
    fn init() -> Result<(), String> {
        host::log(LogLevel::Info, "hello-plugin: initialized");

        // Store initialization marker
        let marker = b"initialized";
        host::kv_set("init_marker", marker)
            .map_err(|e| format!("Failed to store marker: {}", e))?;

        Ok(())
    }

    fn on_event(event: RoomEvent) -> Result<(), String> {
        let msg = format!("hello-plugin: event from {}: {}", event.sender, event.body);
        host::log(LogLevel::Debug, &msg);

        // Handle infinite loop test case (stopped by fuel)
        if event.body == "loop" {
            loop {
                core::hint::spin_loop();
            }
        }

        // Handle memory hog test case (stopped by 32 MiB memory cap)
        if event.body == "allocate_hog" {
            let mut hog = Vec::with_capacity(40 * 1024 * 1024); // 40 MiB > 32 MiB cap
            hog.resize(40 * 1024 * 1024, 42u8);
            if !hog.is_empty() {
                return Ok(());
            }
        }

        // Handle send-message test case (gated by room:send capability)
        if event.body == "ping" {
            host::send_message(&event.room_id, "pong")
                .map_err(|e| format!("send_message error: {}", e))?;
            return Ok(());
        }

        // Count events in KV
        let count_key = format!("event_count_{}", event.room_id);
        let current = host::kv_get(&count_key)
            .map_err(|e| format!("KV error: {}", e))?
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .unwrap_or(0);

        let next = current + 1;
        host::kv_set(&count_key, next.to_string().as_bytes())
            .map_err(|e| format!("KV set error: {}", e))?;

        Ok(())
    }

    fn render_panel() -> Result<String, String> {
        let panel = r#"{"type": "panel", "title": "Hello Plugin", "items": [{"type": "text", "text": "Hello from WASM!"}]}"#;
        Ok(panel.to_string())
    }
}

export!(Component);
