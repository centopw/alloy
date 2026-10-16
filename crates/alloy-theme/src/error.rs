use thiserror::Error;

/// Theme loading and validation errors.
#[derive(Error, Debug)]
pub enum ThemeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Invalid color format: {0}")]
    InvalidColor(String),

    #[error("Unknown key in theme: {0}")]
    UnknownKey(String),

    #[error("Missing required token: {0}")]
    MissingToken(String),
    #[error(
        "Invalid contrast ratio for {text} on {background}: {ratio:.2} (minimum {minimum:.2} required)"
    )]
    ContrastFailed {
        text: String,
        background: String,
        ratio: f64,
        minimum: f64,
    },

    #[error("Theme validation error: {0}")]
    Validation(String),
}

/// Result type for theme operations.
pub type Result<T, E = ThemeError> = std::result::Result<T, E>;
