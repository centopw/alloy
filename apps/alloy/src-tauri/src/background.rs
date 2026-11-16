//! Chat background pictures: writes a decoded picture under `<app_data>/media/backgrounds/` and
//! removes one by name. The asset protocol scope in `tauri.conf.json` already serves
//! `$APPDATA/media/**`, so the page loads a stored picture with the same `convertFileSrc` path as
//! cached media.

use std::io;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use tauri::{AppHandle, Manager};

/// Subdirectory of the app data dir the asset protocol serves.
const MEDIA_DIR: &str = "media";
/// Where background pictures live inside the media dir.
const BACKGROUND_DIR: &str = "backgrounds";
/// Largest accepted picture, matching the media cache's practical ceiling.
const MAX_BYTES: usize = 12 * 1024 * 1024;
/// Longest accepted extension, matching `save.rs`'s sanitizer.
const MAX_EXTENSION: usize = 5;

/// Stores a chat background picture and returns its file name.
///
/// `data` is standard base64 without a `data:` prefix. The file is written as
/// `<app_data>/media/backgrounds/<uuid>.<extension>`, and only the name goes back to the page.
#[tauri::command]
pub async fn save_background(
    app: AppHandle,
    data: String,
    extension: String,
) -> Result<String, String> {
    let bytes = STANDARD
        .decode(data.as_bytes())
        .map_err(|err| format!("invalid picture data: {err}"))?;
    if bytes.is_empty() {
        return Err("the picture is empty".to_owned());
    }
    if bytes.len() > MAX_BYTES {
        return Err("the picture is larger than 12 MiB".to_owned());
    }
    let extension = sanitize_extension(&extension);
    tauri::async_runtime::spawn_blocking(move || write_background(&app, &bytes, &extension))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

/// Removes a stored background. A name that is already gone resolves `Ok(())`.
#[tauri::command]
pub async fn delete_background(app: AppHandle, file_name: String) -> Result<(), String> {
    validate_file_name(&file_name).map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = background_dir(&app)?.join(&file_name);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

fn background_dir(app: &AppHandle) -> io::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| io::Error::other(err.to_string()))?
        .join(MEDIA_DIR)
        .join(BACKGROUND_DIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_background(app: &AppHandle, bytes: &[u8], extension: &str) -> io::Result<String> {
    let dir = background_dir(app)?;
    let file_name = format!("{}.{extension}", uuid::Uuid::new_v4());
    std::fs::write(dir.join(&file_name), bytes)?;
    Ok(file_name)
}

/// Keeps up to [`MAX_EXTENSION`] lowercase letters and digits, so an extension can never leave the
/// directory. Falls back to `png`, which the page can always render.
fn sanitize_extension(raw: &str) -> String {
    let cleaned: String = raw
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_EXTENSION)
        .collect();
    if cleaned.is_empty() {
        "png".to_owned()
    } else {
        cleaned
    }
}

/// Accepts only a plain file name with a short alphanumeric suffix: no separators, no `..`, nothing
/// that could climb out of the backgrounds directory.
fn validate_file_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("the background name is empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("the background name must be a plain file name");
    }
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return Err("the background name has no extension");
    };
    if stem.is_empty() {
        return Err("the background name has no stem");
    }
    if extension.is_empty()
        || extension.len() > MAX_EXTENSION
        || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err("the background name has an invalid extension");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_extensions() {
        assert_eq!(sanitize_extension("PNG"), "png");
        assert_eq!(sanitize_extension("../jpg"), "jpg");
        assert_eq!(sanitize_extension("j peg*"), "jpeg");
        assert_eq!(sanitize_extension("abcdefg"), "abcde");
        assert_eq!(sanitize_extension(""), "png");
        assert_eq!(sanitize_extension("/\\."), "png");
    }

    #[test]
    fn rejects_names_that_could_leave_the_directory() {
        for bad in [
            "",
            "../escape.png",
            "nested/file.png",
            "nested\\file.png",
            "..png",
            ".png",
            "noextension",
            "image.toolong",
            "image.",
            "image.p/ng",
        ] {
            assert!(validate_file_name(bad).is_err(), "{bad:?} was accepted");
        }
        assert!(validate_file_name("0f3f2c9a-1c4a-4b8e-9d21-2f6c1f0a9b7e.png").is_ok());
    }
}
