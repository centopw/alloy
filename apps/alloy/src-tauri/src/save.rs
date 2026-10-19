//! Desktop "Save as" for cached media files, through the native save dialog.

use std::io;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::pick::sanitize_file_name;

/// Directory under the app data dir where `alloy-core` caches fetched media.
const MEDIA_CACHE_DIR: &str = "media_cache";

/// Copies a cached media file to a location the user picks. Resolves `Ok(false)` when the dialog is
/// cancelled. Only files inside the media cache are accepted.
#[tauri::command]
pub async fn save_media(
    app: AppHandle,
    local_path: String,
    suggested_name: String,
) -> Result<bool, String> {
    let cache_root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(MEDIA_CACHE_DIR);
    let source = contained_file(&cache_root, Path::new(&local_path)).map_err(|e| e.to_string())?;
    let name = sanitize_file_name(&suggested_name);
    tauri::async_runtime::spawn_blocking(move || save_to_chosen_path(&app, &source, &name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn save_to_chosen_path(app: &AppHandle, source: &Path, name: &str) -> io::Result<bool> {
    let Some(target) = app.dialog().file().set_file_name(name).blocking_save_file() else {
        return Ok(false);
    };
    let dest = target
        .into_path()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    std::fs::copy(source, dest)?;
    Ok(true)
}

/// Returns the canonical path of `requested` only when it is a regular file inside `cache_root`.
/// Symlinks are resolved first, so a link planted in the cache cannot reach files outside it.
fn contained_file(cache_root: &Path, requested: &Path) -> io::Result<PathBuf> {
    let root = cache_root.canonicalize()?;
    let file = requested.canonicalize()?;
    if file.starts_with(&root) && file.is_file() {
        Ok(file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "file is outside the media cache",
        ))
    }
}

/// Directory under the app data dir where voice messages and other temp files are stored.
pub const TEMP_VOICE_DIR: &str = "temp_voice";

/// Maximum size for a temporary file (64 MiB).
const MAX_TEMP_FILE_SIZE: usize = 64 * 1024 * 1024;

/// Keeps up to five lowercase letters and digits, falling back to `ogg`, so a name can never leave the directory.
fn sanitize_extension(raw: &str) -> String {
    let cleaned: String = raw
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(5)
        .collect();
    if cleaned.is_empty() {
        "ogg".to_owned()
    } else {
        cleaned
    }
}

/// Decodes the base64 clip sent by the web layer and enforces the size bound.
fn decode_clip(data: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    if data.len() > MAX_TEMP_FILE_SIZE / 3 * 4 + 4 {
        return Err("the recording is too large".to_owned());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("the recording is not valid: {e}"))?;
    if bytes.is_empty() {
        return Err("the recording is empty".to_owned());
    }
    if bytes.len() > MAX_TEMP_FILE_SIZE {
        return Err("the recording is too large".to_owned());
    }
    Ok(bytes)
}

/// Writes `bytes` to a new randomly named file in `dir` (created when missing), prunes old files and returns the absolute path.
fn store_clip(dir: &Path, bytes: &[u8], extension: &str) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "{}.{}",
        uuid::Uuid::new_v4(),
        sanitize_extension(extension)
    ));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let _ = prune_old_files(dir);
    path.into_os_string()
        .into_string()
        .map_err(|_| "the temp path is not valid UTF-8".to_owned())
}

/// Stores a recorded voice clip (base64, at most 64 MiB decoded) in `temp_voice/` and returns its absolute path
/// for `SendVoice`. The directory is registered with the engine's attachment allow-list at startup.
/// Files older than 24 hours are pruned on every call.
#[tauri::command]
pub async fn write_temp_file(
    app: AppHandle,
    data: String,
    extension: String,
) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(TEMP_VOICE_DIR);
    tauri::async_runtime::spawn_blocking(move || store_clip(&dir, &decode_clip(&data)?, &extension))
        .await
        .map_err(|e| e.to_string())?
}

/// Remove temp files older than 24 hours.
fn prune_old_files(dir: &Path) -> io::Result<()> {
    let now = std::time::SystemTime::now();
    let cutoff = std::time::Duration::from_secs(24 * 3600);

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let is_old = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|mod_time| now.duration_since(mod_time).ok())
                .is_some_and(|elapsed| elapsed > cutoff);
            if is_old {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Fresh scratch directory under the system temp dir, containing a `media_cache` directory.
    fn scratch(tag: &str) -> (PathBuf, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("alloy-save-{tag}-{}-{nanos}", std::process::id()));
        let cache = base.join(MEDIA_CACHE_DIR);
        std::fs::create_dir_all(&cache).unwrap();
        (base, cache)
    }

    #[test]
    fn accepts_regular_file_inside_cache() {
        let (base, cache) = scratch("inside");
        let file = cache.join("abc.png");
        std::fs::write(&file, b"png").unwrap();
        let resolved = contained_file(&cache, &file).unwrap();
        assert_eq!(resolved, file.canonicalize().unwrap());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rejects_file_outside_cache() {
        let (base, cache) = scratch("outside");
        let file = base.join("secret.txt");
        std::fs::write(&file, b"secret").unwrap();
        assert!(contained_file(&cache, &file).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rejects_sibling_whose_name_shares_the_cache_prefix() {
        let (base, cache) = scratch("prefix");
        let sibling = base.join("media_cache2");
        std::fs::create_dir_all(&sibling).unwrap();
        let file = sibling.join("x.bin");
        std::fs::write(&file, b"x").unwrap();
        assert!(contained_file(&cache, &file).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rejects_directory_and_missing_paths() {
        let (base, cache) = scratch("dirs");
        let sub = cache.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(contained_file(&cache, &sub).is_err());
        assert!(contained_file(&cache, &cache.join("missing.png")).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_cache() {
        let (base, cache) = scratch("symlink");
        let outside = base.join("outside.txt");
        std::fs::write(&outside, b"secret").unwrap();
        let link = cache.join("link.png");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(contained_file(&cache, &link).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn prune_keeps_recent_files() {
        let (base, _) = scratch("prune");
        let temp_dir = base.join(TEMP_VOICE_DIR);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let recent = temp_dir.join("recent.ogg");
        std::fs::write(&recent, b"recent").unwrap();
        assert!(prune_old_files(&temp_dir).is_ok());
        assert!(recent.exists(), "a file written just now must survive");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn extension_is_limited_to_five_lowercase_alphanumerics() {
        assert_eq!(sanitize_extension("Ogg_2023!@#"), "ogg20");
        assert_eq!(sanitize_extension("../../etc"), "etc");
        assert_eq!(sanitize_extension("!@#$%"), "ogg");
        assert_eq!(sanitize_extension(""), "ogg");
    }

    #[test]
    fn decodes_a_clip_and_refuses_bad_or_oversized_data() {
        assert_eq!(decode_clip("aGVsbG8=").unwrap(), b"hello");
        assert!(decode_clip("not base64 !!").is_err());
        assert!(
            decode_clip("").is_err(),
            "an empty clip is not a voice message"
        );
        use base64::Engine as _;
        let too_big =
            base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_TEMP_FILE_SIZE + 1]);
        assert!(decode_clip(&too_big).is_err());
    }

    #[test]
    fn writes_the_clip_under_the_given_directory_and_returns_its_absolute_path() {
        let (base, _) = scratch("write");
        let dir = base.join(TEMP_VOICE_DIR);
        let path = store_clip(&dir, b"abc", "webm").unwrap();
        assert!(Path::new(&path).is_absolute());
        assert!(path.ends_with(".webm"));
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
