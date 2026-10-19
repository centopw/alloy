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
}
