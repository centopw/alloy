//! Small device-local JSON files the engine owns outside the database (tag catalogue, drafts).
//!
//! Writes go through a temporary file that is flushed and renamed over the real one, so a crash
//! mid-write leaves the previous readable file in place instead of a truncated one.

use std::io::Write;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::EngineError;

/// Reads a JSON file, returning `None` when it does not exist or is empty.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, EngineError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(EngineError::internal_error(format!(
                "cannot read {}: {err}",
                path.display()
            )));
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text).map(Some).map_err(|err| {
        EngineError::internal_error(format!("{} is not valid JSON: {err}", path.display()))
    })
}

/// Writes a JSON file atomically.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), EngineError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            EngineError::internal_error(format!("cannot create {}: {err}", parent.display()))
        })?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|err| {
        EngineError::internal_error(format!("cannot encode {}: {err}", path.display()))
    })?;
    let temp = path.with_extension("json.tmp");

    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        // Make sure the bytes are on disk before the rename replaces the readable file.
        file.sync_all()?;
        std::fs::rename(&temp, path)
    };
    if let Err(err) = write() {
        // Do not leave a half-written temp file behind when the write or the rename failed.
        let _ = std::fs::remove_file(&temp);
        return Err(EngineError::internal_error(format!(
            "cannot write {}: {err}",
            path.display()
        )));
    }
    Ok(())
}

/// First `max` characters of `text`, for the truncated previews the room list shows.
pub fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
