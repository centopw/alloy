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

/// Reports the notification permission without asking: "granted", "denied" or "prompt" (not decided yet).
#[tauri::command]
pub async fn notify_permission_state(app: AppHandle) -> Result<&'static str, String> {
    Ok(
        match app
            .notification()
            .permission_state()
            .map_err(|e| e.to_string())?
        {
            PermissionState::Granted => "granted",
            PermissionState::Denied => "denied",
            PermissionState::Prompt | PermissionState::PromptWithRationale => "prompt",
        },
    )
}

/// Shows one notification. `silent` omits the sound; desktop plays no sound unless one is named.
/// `room_id` rides along as the notification's extra data, so a click on it can open that room.
#[tauri::command]
pub async fn notify_message(
    app: AppHandle,
    title: String,
    body: String,
    silent: bool,
    room_id: String,
) -> Result<(), String> {
    if !ensure_permission(&app)? {
        return Err("notification permission is not granted".into());
    }
    let mut builder = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .extra("room_id", room_id);
    if !silent {
        builder = builder.sound("default");
    }
    builder.show().map_err(|e| e.to_string())
}

/// Opens the room a notification belongs to when the user clicks it. The web UI listens for the
/// `alloy-open-room` event. Only `tap` (a click on the notification itself) is handled; other
/// actions and notifications without a `room_id` are ignored.
///
/// Mobile delivers taps through the Android bridge instead, so this is registered on desktop only.
#[cfg(desktop)]
pub fn register_click_handler(app: &AppHandle) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    use tauri_plugin_notification::Notification;
    let handle = app.clone();
    app.state::<Notification<tauri::Wry>>()
        .on_action(move |action| {
            if action.action_id() != "tap" {
                return;
            }
            let room_id = action
                .notification()
                .and_then(|n| n.extra().get("room_id"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(room_id) = room_id else {
                return;
            };
            // A tap can arrive while the window is hidden or minimised: bring it back first so the
            // room is visible, not selected in a window nobody can see.
            for window in handle.webview_windows().values() {
                if let Err(err) = window.unminimize().and_then(|()| window.show()).and_then(|()| window.set_focus()) {
                    tracing::warn!(error = %err, "failed to bring the window forward for a notification click");
                }
            }
            if let Err(err) = handle.emit("alloy-open-room", room_id.to_owned()) {
                tracing::warn!(error = %err, "failed to emit alloy-open-room for a notification click");
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(mobile)]
pub fn register_click_handler(_app: &AppHandle) -> Result<(), String> {
    Ok(())
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

/// Opens the real OS settings page for the requested permission on desktop.
/// Fails on Android (which uses its native bridge) and for unknown permissions.
#[tauri::command]
pub fn open_permission_settings(permission: String) -> Result<(), String> {
    match permission.as_str() {
        "notifications" | "microphone" | "location" => {}
        other => return Err(format!("Unsupported permission '{other}'")),
    }

    #[cfg(target_os = "android")]
    {
        Err(
            "open_permission_settings is not supported on Android; use the native bridge"
                .to_string(),
        )
    }

    #[cfg(target_os = "macos")]
    {
        let uri = match permission.as_str() {
            "notifications" => "x-apple.systempreferences:com.apple.preference.notifications",
            "microphone" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            "location" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices"
            }
            _ => unreachable!(),
        };
        std::process::Command::new("open")
            .arg(uri)
            .spawn()
            .map_err(|e| format!("Failed to open system settings: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let uri = match permission.as_str() {
            "notifications" => "ms-settings:notifications",
            "microphone" => "ms-settings:privacy-microphone",
            "location" => "ms-settings:privacy-location",
            _ => unreachable!(),
        };
        std::process::Command::new("cmd")
            .args(["/C", "start", "", uri])
            .spawn()
            .map_err(|e| format!("Failed to open system settings: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let (gnome_args, kde_args): (&[&str], &[&str]) = match permission.as_str() {
            "notifications" => (&["notifications"], &["kcm_notifications"]),
            "microphone" => (&["privacy", "microphone"], &["kcm_privacy"]),
            "location" => (&["privacy", "location"], &["kcm_privacy"]),
            _ => unreachable!(),
        };

        match std::process::Command::new("gnome-control-center")
            .args(gnome_args)
            .spawn()
        {
            Ok(_) => Ok(()),
            Err(gnome_err) => std::process::Command::new("systemsettings")
                .args(kde_args)
                .spawn()
                .map(|_| ())
                .map_err(|kde_err| {
                    format!(
                        "Failed to open settings: gnome-control-center failed ({gnome_err}), systemsettings failed ({kde_err})"
                    )
                }),
        }
    }

    #[cfg(not(any(
        target_os = "android",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        Err("Opening permission settings is not supported on this platform".to_string())
    }
}
