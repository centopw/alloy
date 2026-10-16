use serde::{Deserialize, Serialize};

/// Plugin manifest as defined in `schemas/plugin-manifest.schema.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: Option<String>,
    pub version: String,
    pub api_version: String,
    pub entrypoint: String,
    pub description: Option<String>,
    pub permissions: Vec<PluginCapability>,
    pub contributes: Option<PluginContributions>,
}

/// A capability requested by the plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapability {
    pub name: String,
    pub scope: Option<String>,
    pub reason: Option<String>,
    pub hosts: Option<Vec<String>>,
    pub quota_kb: Option<u32>,
    pub slots: Option<Vec<String>>,
}

/// Declared UI contributions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContributions {
    pub commands: Option<Vec<PluginCommand>>,
    pub panels: Option<Vec<PluginPanel>>,
    pub message_renderers: Option<Vec<PluginMessageRenderer>>,
    pub themes: Option<Vec<PluginTheme>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPanel {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMessageRenderer {
    pub id: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTheme {
    pub id: String,
    pub path: String,
}
