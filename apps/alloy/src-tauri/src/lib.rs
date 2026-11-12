//! Alloy desktop and mobile application shell.

pub mod commands;
mod notify;
mod pick;
mod save;
mod update;

#[cfg(mobile)]
mod tls;

use alloy_engine::context::EngineContext;
use alloy_engine::factory::{ConnectorFactory, DefaultConnectorFactory, MESSENGER_SIDECAR_ENV};
use alloy_engine::router::AppEngine;
use commands::{alloy_invoke, alloy_subscribe};
use std::path::{Path, PathBuf};
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
    data_dir: &Path,
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

/// The Messenger sidecar executable, when this build has one.
///
/// [`MESSENGER_SIDECAR_ENV`] wins so a developer can point at a sidecar built elsewhere; otherwise
/// the sidecar Tauri bundled next to the app is used. A missing sidecar only removes Messenger from
/// the network list (ADR 0016): Telegram keeps working.
fn messenger_sidecar() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(MESSENGER_SIDECAR_ENV) {
        return Some(PathBuf::from(path));
    }
    let path = bundled_sidecar();
    if path.is_none() {
        tracing::info!("no Messenger sidecar next to the executable; Messenger is unavailable");
    }
    path
}

/// Desktop bundles ship the sidecar next to the main executable (`externalBin` in `tauri.conf.json`).
#[cfg(not(target_os = "android"))]
fn bundled_sidecar() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe
        .parent()?
        .join(format!("alloy-meta{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Android ships the sidecar as the native library `liballoymeta.so`, because an app may only
/// execute files from its native library directory. That directory carries an install-specific hash,
/// so its path is read back from the mapped libraries instead of guessed.
#[cfg(target_os = "android")]
fn bundled_sidecar() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    let dir = maps.lines().find_map(|line| {
        let path = line.split_whitespace().last()?;
        if path.ends_with("/liballoy_app_lib.so") {
            std::path::Path::new(path).parent()
        } else {
            None
        }
    })?;
    ["liballoymeta.so", "liballoy-meta.so", "alloy-meta"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The engine reports protocol errors, which are not `std::error::Error`; setup returns boxed
/// errors, so give it a message it can print.
fn engine_error(err: alloy_engine::EngineError) -> std::io::Error {
    std::io::Error::other(format!("{}: {}", err.code, err.message))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(update::PendingUpdate::default());
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        // Tauri forwards a drop to the page and to Rust in the same event-loop turn, and this listener
        // runs inside that turn: the page can only ask for a batch afterwards, so the queue is filled
        // before any invoke is served. Recording here rather than trusting a path the page names is
        // what keeps the engine allowlist no wider than the files the user actually dropped.
        .on_webview_event(|webview, event| {
            let tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event
            else {
                return;
            };
            if let Some(dropped) = webview.try_state::<pick::DroppedPaths>() {
                dropped.record(paths);
            }
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("./data"));

            // Create data dir if not exists
            std::fs::create_dir_all(&data_dir)?;

            let secret_store = open_secret_store(&data_dir)?;
            // The Telegram application pair is the user's own, sealed in this same secret store;
            // the factory reads it on every `ListNetworks`/`BeginLogin`, so a pair entered while
            // the app runs takes effect without a restart (ADR 0016).
            let factory: Arc<dyn ConnectorFactory> = Arc::new(DefaultConnectorFactory::new(
                secret_store.clone(),
                messenger_sidecar(),
            ));
            // The database is opened before the window is shown so the first `ListChats` is served
            // from the stored projection instead of racing the migrations.
            let ctx = tauri::async_runtime::block_on(EngineContext::open(
                data_dir.clone(),
                secret_store,
                factory,
            ))
            .map_err(engine_error)?;

            let temp_voice_path = data_dir.join(save::TEMP_VOICE_DIR);
            std::fs::create_dir_all(&temp_voice_path)?;
            if let Err(err) = ctx.allow_attachment_root(&temp_voice_path) {
                tracing::warn!(error = %err, "cannot allow the voice-clip directory");
            }
            let engine = Arc::new(AppEngine::new(ctx));

            // Account actors reconnect in the background: each one owns a connector and an outbox
            // worker, and a dead session only marks that account for a fresh login.
            let accounts = engine.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = accounts.start().await {
                    tracing::error!(error = %err.message, "cannot load the connected accounts");
                }
            });

            app.manage(AppState { engine });
            app.manage(pick::DroppedPaths::default());
            notify::register_click_handler(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            alloy_invoke,
            alloy_subscribe,
            pick::pick_attachment,
            pick::pick_attachments,
            pick::pick_media_files,
            pick::save_pasted_image,
            pick::import_pasted_image_path,
            pick::take_dropped_attachments,
            pick::attachment_size,
            save::save_media,
            save::write_temp_file,
            update::check_update,
            update::download_update,
            update::apply_update,
            notify::notify_permission,
            notify::notify_permission_state,
            notify::notify_message,
            notify::autostart_get,
            notify::autostart_set,
            notify::open_permission_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
