use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse};
use alloy_theme::{ThemeLoader, ThemeResolver};
use std::fs;
use std::path::PathBuf;

/// Handler for theme and settings operations (`CommandGroup::Settings`).
///
/// Owned by stream: `theme-runtime`
/// Provides a complete vertical slice for theme management and user preferences.
#[derive(Default)]
pub struct SettingsHandler;

impl SettingsHandler {
    pub fn new() -> Self {
        Self
    }

    /// Returns the directory that holds user theme files.
    ///
    /// Release builds read only `<data_dir>/themes`, so files placed in a working directory or
    /// named by an environment variable cannot inject CSS. Debug builds also honour the
    /// development locations from `dev_themes_dir`.
    fn themes_dir(ctx: &EngineContext) -> Option<PathBuf> {
        if let Some(dev) = Self::dev_themes_dir() {
            return Some(dev);
        }
        let dir = ctx.data_dir.join("themes");
        dir.is_dir().then_some(dir)
    }

    /// Development-only theme locations: `ALLOY_THEMES_DIR`, then `packages/themes` in the
    /// current directory or an ancestor.
    #[cfg(debug_assertions)]
    fn dev_themes_dir() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("ALLOY_THEMES_DIR") {
            let path = PathBuf::from(dir);
            if path.is_dir() {
                return Some(path);
            }
        }
        let mut current = std::env::current_dir().ok()?;
        loop {
            let themes_path = current.join("packages/themes");
            if themes_path.is_dir() {
                return Some(themes_path);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    #[cfg(not(debug_assertions))]
    fn dev_themes_dir() -> Option<PathBuf> {
        None
    }

    /// Helper to check if a file is a JSON or JSONC theme file.
    fn is_theme_file(path: &std::path::Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext == "json" || ext == "jsonc")
    }

    /// Theme ids become registry keys and CSS selector fragments, so keep them to a plain slug.
    fn is_valid_theme_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }

    /// Rejects CSS values that could end their declaration or load external resources.
    fn is_safe_css_value(value: &str) -> bool {
        if value.is_empty() || value.len() > 256 {
            return false;
        }
        if value
            .chars()
            .any(|c| c.is_control() || matches!(c, ';' | '{' | '}' | '<' | '>' | '\\'))
        {
            return false;
        }
        let lower = value.to_ascii_lowercase();
        !["url(", "@import", "expression("]
            .iter()
            .any(|bad| lower.contains(bad))
    }

    /// Loads every readable theme file in the themes directory. Files with an invalid id or
    /// that fail to parse are skipped with a warning.
    fn load_workspace_themes(ctx: &EngineContext) -> Vec<alloy_theme::Theme> {
        let Some(themes_dir) = Self::themes_dir(ctx) else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&themes_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| Self::is_theme_file(path))
            .filter_map(|path| match ThemeLoader::load_from_path(&path) {
                Ok(theme) if Self::is_valid_theme_id(&theme.id) => Some(theme),
                Ok(theme) => {
                    tracing::warn!(path = %path.display(), id = %theme.id, "skipping theme with invalid id");
                    None
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping unreadable theme");
                    None
                }
            })
            .collect()
    }

    /// Lists all available themes (built-in + workspace themes).
    async fn list_themes(&self, ctx: &EngineContext) -> Result<Vec<(String, String)>, EngineError> {
        let mut themes = vec![
            ("alloy-light".into(), "Alloy Light".into()),
            ("alloy-dark".into(), "Alloy Dark".into()),
            ("alloy-amoled".into(), "Alloy AMOLED".into()),
        ];

        for theme in Self::load_workspace_themes(ctx) {
            // Avoid duplicates if a workspace theme overrides a builtin
            if !themes.iter().any(|(id, _)| id == &theme.id) {
                themes.push((theme.id, theme.name));
            }
        }

        themes.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(themes)
    }

    /// Resolves a theme and returns its CSS variables scoped.
    async fn get_theme(&self, ctx: &EngineContext, id: &str) -> Result<String, EngineError> {
        if !Self::is_valid_theme_id(id) {
            return Err(EngineError::invalid_argument(format!(
                "Invalid theme id '{}'",
                id
            )));
        }

        let workspace = Self::load_workspace_themes(ctx);
        let mut resolver = ThemeResolver::new();
        for theme in &workspace {
            resolver.register(theme.clone());
        }

        // Resolve the requested theme
        let theme = match id {
            "alloy-light" => alloy_theme::alloy_light(),
            "alloy-dark" => alloy_theme::alloy_dark(),
            "alloy-amoled" => alloy_theme::alloy_amoled(),
            _ => workspace
                .iter()
                .find(|theme| theme.id == id)
                .cloned()
                .ok_or_else(|| EngineError::not_found(format!("Theme '{}' not found", id)))?,
        };

        let resolved = resolver.resolve(&theme).map_err(|e| {
            EngineError::invalid_argument(format!("Failed to resolve theme: {}", e))
        })?;

        // Format CSS scoped by theme ID, matching gen-css style
        let mut css = format!("[data-theme=\"{}\"] {{\n", resolved.id);
        for (var_name, value) in resolved.css_vars() {
            if !Self::is_safe_css_value(&value) {
                return Err(EngineError::invalid_argument(format!(
                    "Theme '{}' has an unsafe value for {}",
                    id, var_name
                )));
            }
            css.push_str(&format!("  {}: {};\n", var_name, value));
        }
        css.push_str("}\n");

        Ok(css)
    }

    /// Gets settings.json from data_dir, or returns empty object if not present.
    fn get_settings_file(&self, ctx: &EngineContext) -> Result<serde_json::Value, EngineError> {
        let settings_path = ctx.data_dir.join("settings.json");
        if !settings_path.exists() {
            return Ok(serde_json::json!({}));
        }

        let content = fs::read_to_string(&settings_path)
            .map_err(|e| EngineError::internal_error(format!("Failed to read settings: {}", e)))?;

        if content.trim().is_empty() {
            return Ok(serde_json::json!({}));
        }

        serde_json::from_str(&content)
            .map_err(|e| EngineError::internal_error(format!("Invalid settings JSON: {}", e)))
    }

    /// Writes settings.json to data_dir, creating parent directories if needed.
    fn write_settings_file(
        &self,
        ctx: &EngineContext,
        settings: serde_json::Value,
    ) -> Result<(), EngineError> {
        fs::create_dir_all(&ctx.data_dir).map_err(|e| {
            EngineError::internal_error(format!("Failed to create data dir: {}", e))
        })?;

        let settings_path = ctx.data_dir.join("settings.json");
        let json_str = serde_json::to_string_pretty(&settings).map_err(|e| {
            EngineError::internal_error(format!("Failed to serialize settings: {}", e))
        })?;

        fs::write(&settings_path, json_str)
            .map_err(|e| EngineError::internal_error(format!("Failed to write settings: {}", e)))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl GroupHandler for SettingsHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Settings
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::ListThemes => {
                let themes = self.list_themes(ctx).await?;
                Ok(EngineResponse::ThemesList { themes })
            }
            EngineCommand::GetTheme { id } => {
                let css = self.get_theme(ctx, &id).await?;
                Ok(EngineResponse::ThemeCss { css })
            }
            EngineCommand::SetActiveTheme { id } => {
                // Validate that the theme exists
                let _ = self.get_theme(ctx, &id).await?;

                // Store in settings
                let mut settings = self.get_settings_file(ctx)?;
                settings["active_theme"] = serde_json::Value::String(id);
                self.write_settings_file(ctx, settings)?;

                Ok(EngineResponse::ThemeSetActive)
            }
            EngineCommand::GetSettings => {
                let settings = self.get_settings_file(ctx)?;
                let settings_json = serde_json::to_string(&settings).map_err(|e| {
                    EngineError::internal_error(format!("Failed to serialize settings: {}", e))
                })?;
                Ok(EngineResponse::SettingsRetrieved { settings_json })
            }
            EngineCommand::SetSetting { key, value_json } => {
                // Parse the value_json
                let value = serde_json::from_str(&value_json).map_err(|e| {
                    EngineError::invalid_argument(format!("Invalid JSON value: {}", e))
                })?;

                // Load, update, and save
                let mut settings = self.get_settings_file(ctx)?;
                if let serde_json::Value::Object(obj) = &mut settings {
                    obj.insert(key.clone(), value);
                } else {
                    return Err(EngineError::internal_error("Settings is not a JSON object"));
                }
                self.write_settings_file(ctx, settings)?;

                Ok(EngineResponse::SettingSet { key })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Settings group",
                name
            ))),
        }
    }
}
