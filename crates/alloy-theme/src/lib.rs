#![forbid(unsafe_code)]
//! # alloy-theme
//!
//! Design token system, theme resolver, and CSS variable generator for the Alloy Matrix client.
//!
//! Themes follow a W3C Design Tokens-flavored JSON/JSONC schema:
//! - **Color**: Surface, background, text, accent, status, bubble, border
//! - **Typography**: Font stacks, size ramp, weights, line heights
//! - **Spacing & Density**: Presets (compact, normal, cozy), scale ramp
//! - **Radii & Elevation**: Standard border radii and drop shadows
//! - **Motion**: Transition durations and prefers-reduced-motion flag
//! - **Layout**: Message layout style (bubble / gutter), avatar shape
//!
//! Supports deep-merging inheritance (`extends`), color validation,
//! strict unknown-key rejection, and WCAG 2.1 AA/AAA contrast reporting.
//!
//! # Example
//!
//! ```no_run
//! use alloy_theme::{ThemeLoader, ThemeResolver};
//!
//! let theme = ThemeLoader::load_from_path("alloy-light.json")?;
//! let resolver = ThemeResolver::new();
//! let resolved = resolver.resolve(&theme)?;
//!
//! // Check WCAG contrast
//! assert!(resolved.contrast_report.passes_aa);
//!
//! // Emit --alloy-* CSS variables
//! let css = resolved.to_css_vars();
//! println!("{}", css);
//! # Ok::<(), alloy_theme::ThemeError>(())
//! ```

pub mod builtin;
pub mod color;
pub mod error;
pub mod loader;
pub mod resolver;
pub mod schema;

pub use builtin::{alloy_amoled, alloy_dark, alloy_light};
pub use color::Color;
pub use error::{Result, ThemeError};
pub use loader::ThemeLoader;
pub use resolver::{ContrastCheck, ContrastReport, ResolvedTheme, ThemeResolver};
pub use schema::{
    AvatarShape, DensityPreset, Keybinding, KeybindingsConfig, MessageStyle, SchemaVersion, Theme,
    ThemeMode, Tokens,
};

#[cfg(test)]
mod tests;
