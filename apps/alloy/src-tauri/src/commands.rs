//! Tauri IPC commands.

use crate::AppState;
use alloy_engine::router::Engine;
use alloy_protocol::{EngineCommand, EngineError, EngineEvent, EngineResponse};
use tauri::ipc::Channel;

#[tauri::command]
pub async fn alloy_invoke(
    command: EngineCommand,
    state: tauri::State<'_, AppState>,
) -> Result<EngineResponse, EngineError> {
    state.engine.handle(command).await
}

#[tauri::command]
pub async fn alloy_subscribe(
    on_event: Channel<EngineEvent>,
    state: tauri::State<'_, AppState>,
) -> Result<(), EngineError> {
    let mut rx = state.engine.sink().subscribe();

    // Spawn a forwarder task from engine broadcast to the Tauri IPC Channel
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if on_event.send(event).is_err() {
                        // Channel closed by receiver; stop forwarding
                        break;
                    }
                }
                // A slow webview missed events. Ending the loop here would freeze the UI until
                // the window reloads, so keep forwarding; the missed diffs are reported.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                    tracing::warn!(dropped, "webview event stream lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Ok(())
}
