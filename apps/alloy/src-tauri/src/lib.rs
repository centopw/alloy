//! Alloy desktop and mobile application shell.

pub mod commands;
mod pick;
mod update;

use alloy_engine::context::EngineContext;
use alloy_engine::router::AppEngine;
use commands::{alloy_invoke, alloy_subscribe};
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub engine: Arc<AppEngine>,
}

/// Picks where account secrets live. Desktop uses the OS keyring, probed at startup so a keyring
/// without a working backend fails instead of holding secrets only in memory. Linux without a
/// running Secret Service falls back to the owner-only file, with a warning. Android and iOS have
/// no keyring backend and always use the owner-only file.
fn open_secret_store(
    data_dir: &std::path::Path,
) -> Result<Arc<dyn alloy_storage::SecretStore>, alloy_storage::StorageError> {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        match alloy_storage::KeyringSecretStore::new("chat.alloy") {
            Ok(store) => return Ok(Arc::new(store)),
            Err(e) if cfg!(target_os = "linux") => {
                tracing::warn!(
                    error = %e,
                    "OS keyring unavailable; storing secrets in an owner-only file"
                );
            }
            Err(e) => return Err(e),
        }
    }
    Ok(Arc::new(alloy_storage::FileSecretStore::new(
        data_dir.join("secrets.json"),
    )))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::PendingUpdate::default());
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("./data"));

            // Create data dir if not exists
            let _ = std::fs::create_dir_all(&data_dir);

            let secret_store = open_secret_store(&data_dir)?;

            let ctx = EngineContext::new(data_dir, secret_store);
            let engine = Arc::new(AppEngine::new(ctx));

            app.manage(AppState { engine });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            alloy_invoke,
            alloy_subscribe,
            pick::pick_attachment,
            update::check_update,
            update::install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
