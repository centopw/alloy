//! Alloy desktop and mobile application shell.

pub mod commands;

use alloy_engine::context::EngineContext;
use alloy_engine::router::AppEngine;
use commands::{alloy_invoke, alloy_subscribe};
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub engine: Arc<AppEngine>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("./data"));

            // Create data dir if not exists
            let _ = std::fs::create_dir_all(&data_dir);

            // Configure Keyring secret store
            let secret_store: Arc<dyn alloy_storage::SecretStore> =
                Arc::new(alloy_storage::KeyringSecretStore::new("chat.alloy"));

            let ctx = EngineContext::new(data_dir, secret_store);
            let engine = Arc::new(AppEngine::new(ctx));

            app.manage(AppState { engine });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![alloy_invoke, alloy_subscribe])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
