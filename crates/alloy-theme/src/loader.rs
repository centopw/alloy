use crate::error::{Result, ThemeError};
use crate::schema::{Keybinding, KeybindingsConfig, Theme};
use std::path::Path;

/// Loads and parses theme and keybinding files from JSON or JSONC format (comments and trailing commas allowed).
pub struct ThemeLoader;

impl ThemeLoader {
    /// Load a theme from a file path (JSON or JSONC with comments).
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Theme> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)?;
        Self::load_from_str(&content)
    }

    /// Load a theme from a string (JSON or JSONC).
    pub fn load_from_str(content: &str) -> Result<Theme> {
        let maybe_theme: Option<Theme> =
            jsonc_parser::parse_to_serde_value(content, &Default::default())
                .map_err(|e| ThemeError::Validation(format!("JSONC syntax error: {}", e)))?;

        maybe_theme.ok_or_else(|| ThemeError::Validation("Empty JSONC input".into()))
    }

    /// Load keybindings from a file path (JSON or JSONC).
    pub fn load_keybindings_from_path<P: AsRef<Path>>(path: P) -> Result<Vec<Keybinding>> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)?;
        Self::load_keybindings_from_str(&content)
    }

    /// Load keybindings from a string (accepts either `[{ key, command, when? }]` or `{ bindings: [...] }`).
    pub fn load_keybindings_from_str(content: &str) -> Result<Vec<Keybinding>> {
        let value: Option<serde_json::Value> =
            jsonc_parser::parse_to_serde_value(content, &Default::default())
                .map_err(|e| ThemeError::Validation(format!("JSONC syntax error: {}", e)))?;

        let val = value.ok_or_else(|| ThemeError::Validation("Empty JSONC input".into()))?;

        // Attempt as raw array first
        if let Ok(bindings) = serde_json::from_value::<Vec<Keybinding>>(val.clone()) {
            return Ok(bindings);
        }

        // Attempt as KeybindingsConfig object
        let config: KeybindingsConfig =
            serde_json::from_value(val).map_err(ThemeError::JsonParse)?;
        Ok(config.bindings)
    }
}
