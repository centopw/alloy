//! Application credentials and client identification for Telegram's MTProto.

use std::fmt;

/// Environment variable carrying the Telegram `api_id`.
pub const API_ID_ENV: &str = "ALLOY_TELEGRAM_API_ID";
/// Environment variable carrying the Telegram `api_hash`.
pub const API_HASH_ENV: &str = "ALLOY_TELEGRAM_API_HASH";

/// What Telegram is told about the client and the account it belongs to.
///
/// `api_id`/`api_hash` identify the *application*, not the user: Alloy ships its own pair and
/// injects it at build time. They are read from the environment
/// ([`TelegramConfig::from_env`]) because they are never hard-coded in the source tree, and the
/// `api_hash` is redacted from [`fmt::Debug`] because it is a credential.
#[derive(Clone, PartialEq, Eq)]
pub struct TelegramConfig {
    /// `api_id` from <https://my.telegram.org>.
    pub api_id: i32,
    /// `api_hash` paired with [`Self::api_id`]. A credential; never log it.
    pub api_hash: String,
    /// Device model reported to Telegram (`initConnection.device_model`).
    pub device_model: String,
    /// Operating system reported to Telegram (`initConnection.system_version`).
    pub system_version: String,
    /// Application version reported to Telegram (`initConnection.app_version`).
    pub app_version: String,
    /// Language code reported to Telegram (`initConnection.lang_code`).
    pub lang_code: String,
}

impl TelegramConfig {
    /// Builds a configuration with Alloy's defaults for the client fields.
    pub fn new(api_id: i32, api_hash: impl Into<String>) -> Self {
        Self {
            api_id,
            api_hash: api_hash.into(),
            device_model: "Alloy".to_string(),
            system_version: std::env::consts::OS.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            lang_code: "en".to_string(),
        }
    }

    /// Reads [`API_ID_ENV`] and [`API_HASH_ENV`].
    ///
    /// The credentials are never written into the repository: the app injects them into the
    /// process environment at build or launch time.
    pub fn from_env() -> Result<Self, ConfigError> {
        let raw_id =
            std::env::var(API_ID_ENV).map_err(|_| ConfigError::Missing { name: API_ID_ENV })?;
        let api_id = raw_id
            .trim()
            .parse::<i32>()
            .map_err(|_| ConfigError::Invalid {
                name: API_ID_ENV,
                value: raw_id,
            })?;
        let api_hash =
            std::env::var(API_HASH_ENV).map_err(|_| ConfigError::Missing { name: API_HASH_ENV })?;
        let api_hash = api_hash.trim();
        if api_hash.is_empty() {
            return Err(ConfigError::Missing { name: API_HASH_ENV });
        }
        Ok(Self::new(api_id, api_hash))
    }
}

impl fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("api_id", &self.api_id)
            .field("api_hash", &"<redacted>")
            .field("device_model", &self.device_model)
            .field("system_version", &self.system_version)
            .field("app_version", &self.app_version)
            .field("lang_code", &self.lang_code)
            .finish()
    }
}

/// Why a [`TelegramConfig`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The environment variable is absent or empty.
    #[error("{name} is not set")]
    Missing {
        /// Name of the missing variable.
        name: &'static str,
    },
    /// The variable is set but its value is not usable.
    #[error("{name} is not valid: {value}")]
    Invalid {
        /// Name of the invalid variable.
        name: &'static str,
        /// The value that was rejected.
        value: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fills_client_defaults() {
        let config = TelegramConfig::new(42, "hash");

        assert_eq!(config.api_id, 42);
        assert_eq!(config.api_hash, "hash");
        assert_eq!(config.device_model, "Alloy");
        assert_eq!(config.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!config.lang_code.is_empty());
    }

    #[test]
    fn debug_redacts_the_api_hash() {
        let config = TelegramConfig::new(42, "6f1c2d3e-secret");

        let rendered = format!("{config:?}");

        assert!(!rendered.contains("secret"), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
        assert!(rendered.contains("42"), "{rendered}");
    }
}
