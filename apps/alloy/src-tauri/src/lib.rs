//! Alloy desktop and mobile application shell.

pub mod commands;
pub mod engine;

use commands::{alloy_invoke, alloy_subscribe};
use engine::{Engine, RouterEngine};
use std::sync::Arc;
use tauri::Manager;
use tauri::ipc::Channel;

pub struct AppState {
    pub engine: Arc<dyn Engine>,
    pub data_dir: std::path::PathBuf,
    pub secret_store: Box<dyn alloy_storage::SecretStore>,
    pub subscribers: Arc<tokio::sync::Mutex<Vec<Channel<alloy_protocol::EngineEvent>>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("./data"));

            // Create data dir if not exists
            let _ = std::fs::create_dir_all(&data_dir);

            // Configure Keyring secret store
            let secret_store: Box<dyn alloy_storage::SecretStore> =
                Box::new(alloy_storage::KeyringSecretStore::new("chat.alloy"));

            let subscribers = Arc::new(tokio::sync::Mutex::new(Vec::new()));
            let engine = Arc::new(RouterEngine::new());

            app.manage(AppState {
                engine,
                data_dir,
                secret_store,
                subscribers,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![alloy_invoke, alloy_subscribe])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
