//! Application credentials and client identification for Telegram's MTProto.

use std::fmt;

/// Environment variable carrying the Telegram `api_id`.
pub const API_ID_ENV: &str = "ALLOY_TELEGRAM_API_ID";
/// Environment variable carrying the Telegram `api_hash`.
pub const API_HASH_ENV: &str = "ALLOY_TELEGRAM_API_HASH";

/// What Telegram is told about the client and the account it belongs to.
///
/// `api_id`/`api_hash` identify the *application*, not the user. Nothing is compiled into the
/// binary: the app asks for the pair once, seals it in the OS keyring and builds this value from
/// it. [`TelegramConfig::from_env`] exists only as a developer override. The `api_hash` is
/// redacted from [`fmt::Debug`] because it is a credential.
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

    /// Reads [`API_ID_ENV`] and [`API_HASH_ENV`] from the process environment.
    ///
    /// This is a developer override only; people using the app enter the pair in Settings and it
    /// is sealed in the OS keyring. A half-set environment (one variable exported, the other not)
    /// is reported instead of being treated as unset, because that is a mistake worth seeing.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// [`Self::from_env`] with the lookup injected, so the rules are testable without mutating the
    /// process environment. A blank value counts as unset.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let api_id = lookup(API_ID_ENV).filter(|value| !value.trim().is_empty());
        let api_hash = lookup(API_HASH_ENV).filter(|value| !value.trim().is_empty());
        if api_id.is_none() && api_hash.is_none() {
            return Err(ConfigError::NotConfigured);
        }
        Self::parse_pair(api_id, api_hash)
    }

    /// Parses and validates one pair; the only place those rules exist.
    fn parse_pair(raw_id: Option<String>, raw_hash: Option<String>) -> Result<Self, ConfigError> {
        let raw_id = raw_id.ok_or(ConfigError::Missing { name: API_ID_ENV })?;
        let api_id = raw_id
            .trim()
            .parse::<i32>()
            .map_err(|_| ConfigError::Invalid {
                name: API_ID_ENV,
                value: raw_id.trim().to_string(),
            })?;
        let raw_hash = raw_hash.ok_or(ConfigError::Missing { name: API_HASH_ENV })?;
        let api_hash = raw_hash.trim();
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
    /// Neither the process environment nor the app's sealed setting supplied an application pair.
    #[error(
        "no Telegram application is configured: enter the api_id/api_hash pair in the app \
         (Settings -> Networks -> Telegram application), or register one at \
         https://my.telegram.org/apps and export ALLOY_TELEGRAM_API_ID and \
         ALLOY_TELEGRAM_API_HASH for a development build"
    )]
    NotConfigured,
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

    #[test]
    fn a_complete_environment_is_used() {
        let config = TelegramConfig::from_lookup(|name| match name {
            API_ID_ENV => Some("11".to_string()),
            API_HASH_ENV => Some("0123456789abcdef0123456789abcdef".to_string()),
            _ => None,
        })
        .expect("the pair is complete");

        assert_eq!(config.api_id, 11);
        assert_eq!(config.api_hash, "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn blank_values_count_as_unset() {
        assert_eq!(
            TelegramConfig::from_lookup(|_| Some("  ".to_string())),
            Err(ConfigError::NotConfigured)
        );
    }

    #[test]
    fn no_pair_at_all_is_not_configured() {
        assert_eq!(
            TelegramConfig::from_lookup(|_| None),
            Err(ConfigError::NotConfigured)
        );
    }

    #[test]
    fn a_non_numeric_api_id_is_invalid() {
        let err = TelegramConfig::from_lookup(|name| match name {
            API_ID_ENV => Some("not-a-number".to_string()),
            API_HASH_ENV => Some("0123456789abcdef0123456789abcdef".to_string()),
            _ => None,
        })
        .expect_err("a non-numeric api_id cannot be used");

        assert!(
            matches!(
                err,
                ConfigError::Invalid {
                    name: API_ID_ENV,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_half_set_environment_is_reported() {
        let err =
            TelegramConfig::from_lookup(|name| (name == API_ID_ENV).then(|| "66".to_string()))
                .expect_err("the pair is incomplete");

        assert!(
            matches!(err, ConfigError::Missing { name: API_HASH_ENV }),
            "{err:?}"
        );
    }

    #[test]
    fn not_configured_names_the_registration_page() {
        let message = ConfigError::NotConfigured.to_string();

        assert!(
            message.contains("https://my.telegram.org/apps"),
            "{message}"
        );
        assert!(message.contains(API_ID_ENV), "{message}");
        assert!(message.contains(API_HASH_ENV), "{message}");
    }
}
