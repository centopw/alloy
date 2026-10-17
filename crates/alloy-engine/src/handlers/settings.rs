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

    /// Locates the workspace `packages/themes` directory.
    fn find_themes_dir(&self) -> Option<PathBuf> {
        // 1. Check ALLOY_THEMES_DIR environment variable
        if let Ok(dir) = std::env::var("ALLOY_THEMES_DIR") {
            let path = PathBuf::from(dir);
            if path.exists() {
                return Some(path);
            }
        }

        // 2. Try to find `packages/themes` relative to current directory or ancestors
        let mut current = std::env::current_dir().ok()?;
        loop {
            let themes_path = current.join("packages/themes");
            if themes_path.exists() && themes_path.is_dir() {
                return Some(themes_path);
            }

            if !current.pop() {
                break;
            }
        }

        // 3. Not found, built-in themes always available
        None
    }

    /// Helper to check if a file is a JSON or JSONC theme file.
    fn is_theme_file(path: &std::path::Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext == "json" || ext == "jsonc")
    }

    /// Lists all available themes (built-in + workspace themes).
    async fn list_themes(&self) -> Result<Vec<(String, String)>, EngineError> {
        let mut themes = vec![
            ("alloy-light".into(), "Alloy Light".into()),
            ("alloy-dark".into(), "Alloy Dark".into()),
            ("alloy-amoled".into(), "Alloy AMOLED".into()),
        ];

        // Load any workspace themes from packages/themes/
        if let Some(themes_dir) = self.find_themes_dir()
            && let Ok(entries) = fs::read_dir(&themes_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if Self::is_theme_file(&path)
                    && let Ok(theme) = ThemeLoader::load_from_path(&path)
                {
                    // Avoid duplicates if a workspace theme overrides a builtin
                    if !themes.iter().any(|(id, _)| id == &theme.id) {
                        themes.push((theme.id, theme.name));
                    }
                }
            }
        }

        themes.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(themes)
    }

    /// Resolves a theme and returns its CSS variables scoped.
    async fn get_theme(&self, id: &str) -> Result<String, EngineError> {
        let mut resolver = ThemeResolver::new();

        // Load and register all workspace themes
        if let Some(themes_dir) = self.find_themes_dir()
            && let Ok(entries) = fs::read_dir(&themes_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if Self::is_theme_file(&path)
                    && let Ok(theme) = ThemeLoader::load_from_path(&path)
                {
                    resolver.register(theme);
                }
            }
        }
        // Resolve the requested theme
        let theme = if id == "alloy-light" {
            alloy_theme::alloy_light()
        } else if id == "alloy-dark" {
            alloy_theme::alloy_dark()
        } else if id == "alloy-amoled" {
            alloy_theme::alloy_amoled()
        } else {
            // Try to find in workspace themes
            self.find_workspace_theme(id)?
        };

        let resolved = resolver.resolve(&theme).map_err(|e| {
            EngineError::invalid_argument(format!("Failed to resolve theme: {}", e))
        })?;

        // Format CSS scoped by theme ID, matching gen-css style
        let mut css = format!("[data-theme=\"{}\"] {{\n", resolved.id);
        for (var_name, value) in resolved.css_vars() {
            css.push_str(&format!("  {}: {};\n", var_name, value));
        }
        css.push_str("}\n");

        Ok(css)
    }

    /// Searches workspace themes directory for a specific theme by ID.
    fn find_workspace_theme(&self, id: &str) -> Result<alloy_theme::Theme, EngineError> {
        self.find_themes_dir()
            .and_then(|themes_dir| {
                fs::read_dir(&themes_dir).ok().and_then(|entries| {
                    entries.flatten().find_map(|entry| {
                        let path = entry.path();
                        if Self::is_theme_file(&path) {
                            ThemeLoader::load_from_path(&path)
                                .ok()
                                .filter(|theme| theme.id == id)
                        } else {
                            None
                        }
                    })
                })
            })
            .ok_or_else(|| EngineError::not_found(format!("Theme '{}' not found", id)))
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
                let themes = self.list_themes().await?;
                Ok(EngineResponse::ThemesList { themes })
            }
            EngineCommand::GetTheme { id } => {
                let css = self.get_theme(&id).await?;
                Ok(EngineResponse::ThemeCss { css })
            }
            EngineCommand::SetActiveTheme { id } => {
                // Validate that the theme exists
                let _ = self.get_theme(&id).await?;

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
