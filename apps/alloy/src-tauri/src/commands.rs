//! Tauri IPC commands.

use crate::AppState;
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
    let mut subs = state.subscribers.lock().await;
    subs.push(on_event);
    Ok(())
}
