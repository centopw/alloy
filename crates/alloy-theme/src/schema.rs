use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// Theme mode (light, dark, or amoled pure black).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    Amoled,
}

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Light => write!(f, "light"),
            Self::Dark => write!(f, "dark"),
            Self::Amoled => write!(f, "amoled"),
        }
    }
}

/// Spacing and layout density preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DensityPreset {
    Compact,
    Normal,
    Cozy,
}

impl fmt::Display for DensityPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compact => write!(f, "compact"),
            Self::Normal => write!(f, "normal"),
            Self::Cozy => write!(f, "cozy"),
        }
    }
}

/// Message layout style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStyle {
    Bubble,
    Gutter,
}

impl fmt::Display for MessageStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bubble => write!(f, "bubble"),
            Self::Gutter => write!(f, "gutter"),
        }
    }
}

/// Avatar display shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AvatarShape {
    Circle,
    Rounded,
    Square,
}

impl fmt::Display for AvatarShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Circle => write!(f, "circle"),
            Self::Rounded => write!(f, "rounded"),
            Self::Square => write!(f, "square"),
        }
    }
}

/// Schema version can be serialized as integer (1) or string ("1.0.0").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaVersion {
    Number(u32),
    String(String),
}

impl Serialize for SchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Number(n) => serializer.serialize_u32(*n),
            Self::String(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SchemaVersionVisitor;
        impl<'de> serde::de::Visitor<'de> for SchemaVersionVisitor {
            type Value = SchemaVersion;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an integer or string version")
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SchemaVersion::Number(value as u32))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SchemaVersion::Number(value as u32))
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SchemaVersion::String(value.to_string()))
            }
        }
        deserializer.deserialize_any(SchemaVersionVisitor)
    }
}

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_accent: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccentColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub away: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BubbleEndpointColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BubbleColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own: Option<BubbleEndpointColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<BubbleEndpointColors>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BorderColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prominent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floating: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserColors {
    #[serde(rename = "1", skip_serializing_if = "Option::is_none")]
    pub c1: Option<String>,
    #[serde(rename = "2", skip_serializing_if = "Option::is_none")]
    pub c2: Option<String>,
    #[serde(rename = "3", skip_serializing_if = "Option::is_none")]
    pub c3: Option<String>,
    #[serde(rename = "4", skip_serializing_if = "Option::is_none")]
    pub c4: Option<String>,
    #[serde(rename = "5", skip_serializing_if = "Option::is_none")]
    pub c5: Option<String>,
    #[serde(rename = "6", skip_serializing_if = "Option::is_none")]
    pub c6: Option<String>,
    #[serde(rename = "7", skip_serializing_if = "Option::is_none")]
    pub c7: Option<String>,
    #[serde(rename = "8", skip_serializing_if = "Option::is_none")]
    pub c8: Option<String>,
    #[serde(rename = "9", skip_serializing_if = "Option::is_none")]
    pub c9: Option<String>,
    #[serde(rename = "10", skip_serializing_if = "Option::is_none")]
    pub c10: Option<String>,
    #[serde(rename = "11", skip_serializing_if = "Option::is_none")]
    pub c11: Option<String>,
    #[serde(rename = "12", skip_serializing_if = "Option::is_none")]
    pub c12: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<AccentColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble: Option<BubbleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<BorderColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrim: Option<String>,
}

// ---------------------------------------------------------------------------
// Typography
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFamilyTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sans: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mono: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontSizeTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xxl: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontWeightTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regular: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semibold: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineHeightTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relaxed: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypographyTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<FontFamilyTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<FontSizeTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<FontWeightTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<LineHeightTokens>,
}

// ---------------------------------------------------------------------------
// Spacing & Density
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityPresetTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_padding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_gap: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacingPresets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact: Option<DensityPresetTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<DensityPresetTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cozy: Option<DensityPresetTokens>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacingTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<DensityPreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presets: Option<SpacingPresets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_padding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_size: Option<String>,
}

// ---------------------------------------------------------------------------
// Radii & Elevation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadiiTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElevationTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lg: Option<String>,
}

// ---------------------------------------------------------------------------
// Motion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionDurationTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionEasingTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ease: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ease_in_out: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<MotionDurationTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<MotionEasingTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduced_motion: Option<bool>,
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_style: Option<MessageStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<AvatarShape>,
}
// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatBackgroundTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ChatBackgroundTokens>,
}

// ---------------------------------------------------------------------------
// Combined Tokens & Theme
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typography: Option<TypographyTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<SpacingTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radii: Option<RadiiTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevation: Option<ElevationTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<ChatTokens>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema_url: Option<String>,
    pub schema_version: SchemaVersion,
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub mode: ThemeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub tokens: Tokens,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<BTreeMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Keybinding Definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keybinding {
    pub key: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindingsConfig {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema_url: Option<String>,
    #[serde(default = "default_schema_version")]
    pub schema_version: SchemaVersion,
    pub bindings: Vec<Keybinding>,
}

fn default_schema_version() -> SchemaVersion {
    SchemaVersion::Number(1)
}
