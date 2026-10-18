//! Desktop and Android notifications, and launch-at-login on desktop.
//!
//! The notification commands are `async` so that the Android permission request, which waits for
//! the user's answer on the UI thread, never blocks the thread that would deliver that answer.

use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// Returns whether notifications may be shown, asking the user first when the state is undecided.
/// Desktop always reports `Granted`; Android reports `Prompt` until the user answers.
fn ensure_permission(app: &AppHandle) -> Result<bool, String> {
    let notification = app.notification();
    match notification.permission_state().map_err(|e| e.to_string())? {
        PermissionState::Granted => Ok(true),
        PermissionState::Denied => Ok(false),
        PermissionState::Prompt | PermissionState::PromptWithRationale => {
            let answer = notification
                .request_permission()
                .map_err(|e| e.to_string())?;
            Ok(answer == PermissionState::Granted)
        }
    }
}

/// Requests notification permission when needed and reports whether it is granted.
#[tauri::command]
pub async fn notify_permission(app: AppHandle) -> Result<bool, String> {
    ensure_permission(&app)
}

/// Shows one notification. `silent` omits the sound; desktop plays no sound unless one is named.
#[tauri::command]
pub async fn notify_message(
    app: AppHandle,
    title: String,
    body: String,
    silent: bool,
) -> Result<(), String> {
    if !ensure_permission(&app)? {
        return Err("notification permission is not granted".into());
    }
    let mut builder = app.notification().builder().title(title).body(body);
    if !silent {
        builder = builder.sound("default");
    }
    builder.show().map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn autostart_enabled(app: &AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| e.to_string())
}

#[cfg(mobile)]
fn autostart_enabled(_app: &AppHandle) -> Result<bool, String> {
    Ok(false)
}

#[cfg(mobile)]
fn set_autostart(_app: &AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        Err("launch at login is not available on this platform".into())
    } else {
        Ok(())
    }
}

/// Reports whether Alloy starts at login. Always `false` on mobile.
#[tauri::command]
pub fn autostart_get(app: AppHandle) -> Result<bool, String> {
    autostart_enabled(&app)
}

/// Enables or disables starting Alloy at login. Fails on mobile when enabling.
#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_autostart(&app, enabled)
}
