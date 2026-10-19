//! Attachment picking. The dialog runs in Rust so that a `content://` URI returned on Android can
//! be copied into the app cache through `tauri-plugin-fs`, which can read it. A plain `std::fs`
//! read cannot. Copies keep the original file name, which the upload uses as the filename.

use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use alloy_engine::{Engine, EngineContext};
use percent_encoding::percent_decode_str;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};

use crate::AppState;

/// Subdirectory of the app cache dir that holds copies of picked files.
const PICKS_DIR: &str = "picks";
/// Pick directories older than this are deleted at the next pick.
const PICK_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
/// File name used when a picked URI yields no usable name.
const FALLBACK_NAME: &str = "attachment";
/// Longest file name kept, in bytes. Common filesystems cap names at 255 bytes.
const MAX_NAME_BYTES: usize = 200;

/// Opens the system file picker and returns the chosen file's local path, or `None` when the
/// user cancels.
///
/// Plain filesystem paths (desktop) are returned unchanged. Content or file URIs (Android) are
/// copied into a unique directory under `<cache_dir>/picks/`, keeping the original file name.
#[tauri::command]
pub async fn pick_attachment(app: AppHandle) -> Result<Option<String>, String> {
    // The dialog blocks and the copy does blocking I/O. Both run off the async runtime.
    tauri::async_runtime::spawn_blocking(move || pick_blocking(&app))
        .await
        .map_err(|e| e.to_string())?
}

/// Opens the picker for several files at once. Empty when the user cancels.
#[tauri::command]
pub async fn pick_attachments(app: AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || pick_many_blocking(&app))
        .await
        .map_err(|e| e.to_string())?
}

fn picks_root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join(PICKS_DIR);
    prune_old_picks(&root, SystemTime::now());
    Ok(root)
}

fn pick_blocking(app: &AppHandle) -> Result<Option<String>, String> {
    let root = picks_root(app)?;
    let Some(picked) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    resolve_picked(app, &root, picked).map(Some)
}

fn pick_many_blocking(app: &AppHandle) -> Result<Vec<String>, String> {
    let root = picks_root(app)?;
    let Some(picked) = app.dialog().file().blocking_pick_files() else {
        return Ok(Vec::new());
    };
    picked
        .into_iter()
        .map(|file| resolve_picked(app, &root, file))
        .collect()
}

/// Makes one picked file sendable: desktop paths are registered as they are, URIs are copied into the pick cache.
fn resolve_picked(app: &AppHandle, picks_root: &Path, picked: FilePath) -> Result<String, String> {
    let state = app.state::<AppState>();
    let ctx = state.engine.context();
    match picked {
        FilePath::Path(path) => {
            // The user picked this file explicitly, so the engine may send exactly this file.
            ctx.allow_attachment_path(&path)
                .map_err(|e| format!("picked file is not readable: {e}"))?;
            path.into_os_string()
                .into_string()
                .map_err(|_| "picked file path is not valid UTF-8".to_owned())
        }
        uri @ FilePath::Url(_) => copy_into_pick_dir(app, ctx, picks_root, uri),
    }
}

/// Copies a picked URI into a fresh, uniquely named directory and returns the copy's path.
fn copy_into_pick_dir(
    app: &AppHandle,
    ctx: &EngineContext,
    picks_root: &Path,
    uri: FilePath,
) -> Result<String, String> {
    let name = file_name_from_uri(&uri);
    let dir = picks_root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create pick directory: {e}"))?;
    // Copies live only under the pick cache, so the engine may read anything beneath it.
    ctx.allow_attachment_root(picks_root)
        .map_err(|e| format!("failed to register pick directory: {e}"))?;
    let dest = dir.join(&name);

    if let Err(err) = copy_uri_to(app, uri, &dest) {
        // Drop the partial copy so it does not linger until the next prune.
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("failed to read the picked file: {err}"));
    }
    dest.into_os_string()
        .into_string()
        .map_err(|_| "pick cache path is not valid UTF-8".to_owned())
}

fn copy_uri_to(app: &AppHandle, uri: FilePath, dest: &Path) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    let mut source = app.fs().open(uri, options)?;
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)?;
    io::copy(&mut source, &mut target)?;
    Ok(())
}

/// Removes pick directories whose last modification is older than [`PICK_RETENTION`].
/// Best-effort: failures are logged and never block the pick.
fn prune_old_picks(picks_root: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(picks_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let expired = entry.file_type().is_ok_and(|t| t.is_dir())
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|modified| {
                    now.duration_since(modified)
                        .is_ok_and(|age| age > PICK_RETENTION)
                });
        if expired && let Err(err) = std::fs::remove_dir_all(&path) {
            tracing::warn!(path = %path.display(), %err, "failed to remove expired pick");
        }
    }
}

/// Best-effort file name for a picked URI.
///
/// Content URIs rarely carry a real name. The last path segment is the closest available
/// source. It is percent-decoded, and only its final component is kept, so no separator can
/// point outside the pick directory.
fn file_name_from_uri(uri: &FilePath) -> String {
    let raw = match uri {
        FilePath::Url(url) => url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned())
            .unwrap_or_default(),
        FilePath::Path(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    sanitize_file_name(&raw)
}

pub(crate) fn sanitize_file_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
                )
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return FALLBACK_NAME.to_owned();
    }
    truncate_to_bytes(trimmed, MAX_NAME_BYTES).to_owned()
}

/// Longest prefix of `s` that is at most `max` bytes and ends on a char boundary.
fn truncate_to_bytes(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_only_last_component() {
        assert_eq!(
            sanitize_file_name("primary:Download/report.pdf"),
            "report.pdf"
        );
        assert_eq!(sanitize_file_name("..\\..\\evil.sh"), "evil.sh");
        assert_eq!(
            sanitize_file_name("invoice\u{202E}fdp.exe"),
            "invoicefdp.exe"
        );
    }

    #[test]
    fn sanitize_falls_back_for_empty_or_dot_names() {
        assert_eq!(sanitize_file_name(""), FALLBACK_NAME);
        assert_eq!(sanitize_file_name(".."), FALLBACK_NAME);
        assert_eq!(sanitize_file_name("a/.."), FALLBACK_NAME);
        assert_eq!(sanitize_file_name("\u{7}"), FALLBACK_NAME);
    }

    #[test]
    fn sanitize_truncates_on_char_boundary() {
        let long = "é".repeat(200); // 400 bytes
        let out = sanitize_file_name(&long);
        assert!(out.len() <= MAX_NAME_BYTES);
        assert!(out.chars().all(|c| c == 'é'));
    }
}
