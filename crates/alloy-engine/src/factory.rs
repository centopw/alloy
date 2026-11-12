//! Which networks this build can connect, and how a connector is built for one of them.
//!
//! The engine never constructs a network connector itself: it asks the factory injected into
//! [`EngineContext`](crate::context::EngineContext). A desktop build injects
//! [`DefaultConnectorFactory`]; the end-to-end tests inject [`FakeConnectorFactory`].
//!
//! The Telegram application pair is the person's own setting: the app asks for it once and seals
//! it in the OS keyring ([`TELEGRAM_APP_SECRET`]). Nothing is compiled in, so an installed binary
//! carries no credential, and the pair can be replaced without a release. The process environment
//! stays as a developer override.
//!
//! Every other network knob is a runtime setting too. A network declares its settings
//! ([`network_setting_specs`]); `SetNetworkSetting` accepts exactly those keys, stores a non-secret
//! value in `<data_dir>/network_settings.json` and seals a secret one in the keyring. The next
//! connector the factory creates is launched with the values in effect, so a network can grow a
//! setting without a protocol change or a new build of the engine's callers.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use alloy_connector::{Connector, ConnectorError};
use alloy_protocol::{Network, NetworkInfoDto, NetworkSettingKind};
use alloy_sidecar::{SidecarConfig, SidecarConnector};
use alloy_storage::{SecretStore, StorageError};
use alloy_telegram::{ConfigError, TelegramConfig, TelegramConnector};
use serde::{Deserialize, Serialize};

use crate::convert;
use crate::error::EngineError;

/// Environment variable naming the Messenger sidecar executable.
pub const MESSENGER_SIDECAR_ENV: &str = "ALLOY_MESSENGER_SIDECAR";

/// Secret under which the user-entered Telegram application pair is sealed.
pub const TELEGRAM_APP_SECRET: &str = "alloy.telegram.app.v1";

/// Named in [`ConnectorError::NotSupported`] when Telegram has no application pair yet.
pub const TELEGRAM_APP_FEATURE: &str = "telegram application credentials";

/// Length of a Telegram `api_hash`: 32 hexadecimal characters.
const API_HASH_LEN: usize = 32;

/// The application pair as the user entered it, sealed as JSON under [`TELEGRAM_APP_SECRET`].
///
/// The wire shape is `{"api_id":<i32>,"api_hash":"<hex>"}`. [`fmt::Debug`] redacts the hash
/// because it is a credential.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramAppRecord {
    /// `api_id` from <https://my.telegram.org/apps>.
    pub api_id: i32,
    /// `api_hash` paired with [`Self::api_id`]. Never log or return it.
    pub api_hash: String,
}

impl TelegramAppRecord {
    /// Validates a user-entered pair: a positive `api_id` and a 32-character hex `api_hash`.
    ///
    /// The message is returned as text so the caller decides the error code; it never contains
    /// the hash.
    pub fn parse(api_id: i32, api_hash: &str) -> Result<Self, String> {
        if api_id <= 0 {
            return Err(format!(
                "the Telegram api_id must be a positive number, got {api_id}"
            ));
        }
        let api_hash = api_hash.trim();
        if api_hash.len() != API_HASH_LEN || !api_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "the Telegram api_hash must be the {API_HASH_LEN}-character hexadecimal value \
                 shown at https://my.telegram.org/apps"
            ));
        }
        Ok(Self {
            api_id,
            api_hash: api_hash.to_string(),
        })
    }

    /// The connector configuration for this pair, with Alloy's client defaults.
    pub fn config(&self) -> TelegramConfig {
        TelegramConfig::new(self.api_id, self.api_hash.clone())
    }
}

impl fmt::Debug for TelegramAppRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramAppRecord")
            .field("api_id", &self.api_id)
            .field("api_hash", &"<redacted>")
            .finish()
    }
}

/// Where the Telegram application pair in effect came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramAppSource {
    /// Entered in the app and sealed in the OS keyring.
    Runtime,
    /// Exported as `ALLOY_TELEGRAM_API_ID`/`ALLOY_TELEGRAM_API_HASH` for a development build.
    Environment,
}

/// A usable Telegram application pair and where it came from.
#[derive(Debug, Clone)]
pub struct ResolvedTelegramApp {
    pub config: TelegramConfig,
    pub source: TelegramAppSource,
}

/// Resolves the pair per call: the sealed runtime value first, then [`TelegramConfig::from_env`],
/// then none.
///
/// A sealed value that cannot be decoded is logged and treated as absent, so a corrupt keyring
/// entry never stops the app; entering the pair again overwrites it.
pub async fn resolve_telegram_app(secrets: &dyn SecretStore) -> Option<ResolvedTelegramApp> {
    match read_telegram_app(secrets).await {
        Ok(Some(record)) => {
            return Some(ResolvedTelegramApp {
                config: record.config(),
                source: TelegramAppSource::Runtime,
            });
        }
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(error = %err.message, "cannot read the sealed Telegram application pair")
        }
    }
    match TelegramConfig::from_env() {
        Ok(config) => Some(ResolvedTelegramApp {
            config,
            source: TelegramAppSource::Environment,
        }),
        Err(err @ ConfigError::NotConfigured) => {
            tracing::debug!(error = %err, "no Telegram application pair is configured");
            None
        }
        Err(err) => {
            tracing::warn!(error = %err, "Telegram credentials in the environment are unusable");
            None
        }
    }
}

/// Reads the sealed pair; `Ok(None)` when none is stored.
pub async fn read_telegram_app(
    secrets: &dyn SecretStore,
) -> Result<Option<TelegramAppRecord>, EngineError> {
    match secrets.get_secret(TELEGRAM_APP_SECRET).await {
        Ok(json) => serde_json::from_str(&json).map(Some).map_err(|err| {
            EngineError::internal_error(format!(
                "the sealed Telegram application pair is not valid JSON: {err}"
            ))
        }),
        Err(StorageError::SecretNotFound(_)) => Ok(None),
        Err(err) => Err(EngineError::internal_error(format!(
            "cannot read the sealed Telegram application pair: {err}"
        ))),
    }
}

/// Seals `record` in the OS keyring, replacing any previous pair.
pub async fn save_telegram_app(
    secrets: &dyn SecretStore,
    record: &TelegramAppRecord,
) -> Result<(), EngineError> {
    let json = serde_json::to_string(record).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot encode the Telegram application pair: {err}"
        ))
    })?;
    secrets
        .set_secret(TELEGRAM_APP_SECRET, &json)
        .await
        .map_err(|err| {
            EngineError::internal_error(format!("cannot seal the Telegram application pair: {err}"))
        })
}

/// Deletes the sealed pair; a later login needs it entered again.
pub async fn clear_telegram_app(secrets: &dyn SecretStore) -> Result<(), EngineError> {
    match secrets.delete_secret(TELEGRAM_APP_SECRET).await {
        Ok(()) | Err(StorageError::SecretNotFound(_)) => Ok(()),
        Err(err) => Err(EngineError::internal_error(format!(
            "cannot delete the sealed Telegram application pair: {err}"
        ))),
    }
}

/// The error a login gets when Telegram has no application pair yet.
///
/// `not_supported` with [`TELEGRAM_APP_FEATURE`] as the reason, and a message that names where the
/// user enters the pair, because this is a setting the person can fix without a new release.
pub fn telegram_app_missing_error() -> EngineError {
    EngineError {
        code: "not_supported".to_string(),
        message: "Telegram needs its application pair first: enter the api_id and api_hash from \
                  https://my.telegram.org/apps in Settings -> Networks -> Telegram application"
            .to_string(),
        reason: Some(TELEGRAM_APP_FEATURE.to_string()),
    }
}

/// File under the data directory holding the non-secret network settings.
pub const NETWORK_SETTINGS_FILE: &str = "network_settings.json";

/// Environment variable carrying the Messenger proxy URL to the sidecar.
pub const MESSENGER_PROXY_ENV: &str = "ALLOY_META_PROXY";

/// Environment variable carrying the Messenger platform to the sidecar.
pub const MESSENGER_PLATFORM_ENV: &str = "ALLOY_META_PLATFORM";

/// The Messenger platform used when the person has not chosen one.
pub const MESSENGER_DEFAULT_PLATFORM: &str = "android";

/// Prefix of the keyring entry a secret network setting is sealed under:
/// `alloy.network.<network>.<key>.v1`.
const NETWORK_SECRET_PREFIX: &str = "alloy.network";

/// How a text setting's value is checked before it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkSettingValidation {
    /// Any non-empty text.
    Free,
    /// An `http://`, `https://` or `socks5://` URL with a host.
    ProxyUrl,
}

/// One allowed value of a [`NetworkSettingKind::Choice`] setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkSettingOptionSpec {
    pub value: &'static str,
    pub label: &'static str,
}

/// One runtime knob a network declares.
///
/// This slice is the whole truth about a network's settings: `SetNetworkSetting` refuses a key that
/// is not declared here, and `ListNetworks` reports exactly these. Adding a knob is a change to
/// this declaration, never to the protocol or to a caller.
#[derive(Debug, Clone, Copy)]
pub struct NetworkSettingSpec {
    /// Key the commands expect.
    pub key: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// How the value is edited and stored.
    pub kind: NetworkSettingKind,
    /// Allowed values for a [`NetworkSettingKind::Choice`] setting; empty otherwise.
    pub options: &'static [NetworkSettingOptionSpec],
    /// Value in effect when nothing is stored. A default is not "set": clearing restores it.
    pub default: Option<&'static str>,
    /// Check the value must pass before it is stored.
    pub validation: NetworkSettingValidation,
    /// Environment variable a child process receives the value in. Only a non-secret setting may
    /// name one: a secret never crosses a process boundary this way.
    pub env: Option<&'static str>,
}

/// Messenger's runtime knobs: the sidecar's proxy and the platform it should present as.
const MESSENGER_SETTINGS: &[NetworkSettingSpec] = &[
    NetworkSettingSpec {
        key: "proxy",
        label: "Proxy URL (http://, https://, socks5://)",
        kind: NetworkSettingKind::Text,
        options: &[],
        default: None,
        validation: NetworkSettingValidation::ProxyUrl,
        env: Some(MESSENGER_PROXY_ENV),
    },
    NetworkSettingSpec {
        key: "platform",
        label: "Platform",
        kind: NetworkSettingKind::Choice,
        options: &[
            NetworkSettingOptionSpec {
                value: "android",
                label: "Messenger for Android",
            },
            NetworkSettingOptionSpec {
                value: "ios",
                label: "Messenger for iOS",
            },
        ],
        default: Some(MESSENGER_DEFAULT_PLATFORM),
        validation: NetworkSettingValidation::Free,
        env: Some(MESSENGER_PLATFORM_ENV),
    },
];

/// The runtime settings `network` declares, in display order.
pub fn network_setting_specs(network: Network) -> &'static [NetworkSettingSpec] {
    match network {
        Network::Messenger => MESSENGER_SETTINGS,
        // Telegram's application pair is a credential, handled by `SetTelegramApp`; the fake is
        // test-only and has no settings.
        Network::Telegram | Network::Fake => &[],
    }
}

/// The declaration of one setting, or `None` when the network does not declare that key.
pub fn network_setting_spec(network: Network, key: &str) -> Option<&'static NetworkSettingSpec> {
    network_setting_specs(network)
        .iter()
        .find(|spec| spec.key == key)
}

/// Non-secret network settings, keyed by network name then setting key.
///
/// The file is written with [`crate::files::write_json`], so a crash mid-write leaves the previous
/// readable file in place. A secret setting never reaches it: those live in the keyring under
/// [`network_setting_secret`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NetworkSettings {
    values: BTreeMap<String, BTreeMap<String, String>>,
}

impl NetworkSettings {
    /// The stored value of `key`, if any.
    pub fn get(&self, network: Network, key: &str) -> Option<&str> {
        self.values
            .get(network_name(network))?
            .get(key)
            .map(String::as_str)
    }

    /// Stores one value; an empty value removes the key rather than storing an empty string.
    pub fn set(&mut self, network: Network, key: &str, value: &str) {
        if value.is_empty() {
            self.remove(network, key);
            return;
        }
        self.values
            .entry(network_name(network).to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// Removes one value, dropping the network entry once it holds nothing.
    pub fn remove(&mut self, network: Network, key: &str) {
        let Some(entry) = self.values.get_mut(network_name(network)) else {
            return;
        };
        entry.remove(key);
        if entry.is_empty() {
            self.values.remove(network_name(network));
        }
    }
}

fn network_name(network: Network) -> &'static str {
    convert::network_label(network)
}

/// `<data_dir>/network_settings.json`.
pub fn network_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(NETWORK_SETTINGS_FILE)
}

/// Reads the settings file; a missing or empty file is an empty set of settings.
pub fn read_network_settings(path: &Path) -> Result<NetworkSettings, EngineError> {
    Ok(crate::files::read_json(path)?.unwrap_or_default())
}

/// Reads the settings for launching or describing a network: an unreadable file is logged and
/// treated as empty, so one corrupt file never stops the app from listing its networks.
pub fn load_network_settings(path: &Path) -> NetworkSettings {
    match read_network_settings(path) {
        Ok(settings) => settings,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err.message,
                "ignoring unreadable network settings"
            );
            NetworkSettings::default()
        }
    }
}

/// The value that applies: the stored one, else the declared default.
pub fn effective_network_setting(
    settings: &NetworkSettings,
    network: Network,
    spec: &NetworkSettingSpec,
) -> Option<String> {
    settings
        .get(network, spec.key)
        .or(spec.default)
        .map(str::to_string)
}

/// Checks `value` against `spec` and returns the trimmed value to store.
///
/// An empty value always means "clear": the caller removes the stored value and the declared
/// default applies again. Everything else is checked here, before anything is written, so a
/// rejected value leaves the current one in place.
pub fn validate_network_setting(
    spec: &NetworkSettingSpec,
    value: &str,
) -> Result<String, EngineError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    match spec.kind {
        NetworkSettingKind::Choice => {
            if spec.options.iter().any(|option| option.value == value) {
                return Ok(value.to_string());
            }
            let allowed: Vec<&str> = spec.options.iter().map(|option| option.value).collect();
            Err(EngineError::invalid_argument(format!(
                "the '{}' setting must be one of {}, got '{value}'",
                spec.key,
                quoted_list(&allowed)
            )))
        }
        NetworkSettingKind::Text | NetworkSettingKind::Secret => match spec.validation {
            NetworkSettingValidation::Free => Ok(value.to_string()),
            NetworkSettingValidation::ProxyUrl => {
                validate_proxy_url(spec.key, value)?;
                Ok(value.to_string())
            }
        },
    }
}

fn quoted_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Rejects anything that is not an `http://`, `https://` or `socks5://` URL with a host.
///
/// The check is intentionally narrow: the value is handed to the sidecar's HTTP stack, so a scheme
/// or authority it cannot use is refused here with a message that names the setting.
fn validate_proxy_url(key: &str, value: &str) -> Result<(), EngineError> {
    let problem = proxy_url_problem(value);
    match problem {
        None => Ok(()),
        Some(problem) => Err(EngineError::invalid_argument(format!(
            "the '{key}' setting must be an http://, https:// or socks5:// URL with a host, \
             but {problem}"
        ))),
    }
}

fn proxy_url_problem(value: &str) -> Option<&'static str> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Some("it has no scheme");
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "socks5"
    ) {
        return Some("the scheme is not http, https or socks5");
    }
    // Authority ends at the first path, query or fragment.
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        // Credentials may precede the host; they are not part of it.
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = match authority.strip_prefix('[') {
        // Bracketed IPv6 literal.
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    if host.is_empty() {
        return Some("it has no host");
    }
    None
}

/// Keyring entry a secret-typed setting is sealed under.
pub fn network_setting_secret(network: Network, key: &str) -> String {
    format!("{NETWORK_SECRET_PREFIX}.{}.{key}.v1", network_name(network))
}

/// One setting change at a time: the file is read, edited and written whole, so two concurrent
/// commands would otherwise lose one of the two values.
static SETTINGS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Stores one validated setting value: in the keyring for a secret setting, in the settings file
/// otherwise. An empty value clears it instead.
pub async fn store_network_setting(
    secrets: &dyn SecretStore,
    path: &Path,
    network: Network,
    spec: &NetworkSettingSpec,
    value: &str,
) -> Result<(), EngineError> {
    let _guard = SETTINGS_LOCK.lock().await;
    if value.is_empty() {
        return clear_setting(secrets, path, network, spec).await;
    }
    if spec.kind == NetworkSettingKind::Secret {
        return secrets
            .set_secret(&network_setting_secret(network, spec.key), value)
            .await
            .map_err(|err| {
                EngineError::internal_error(format!(
                    "cannot seal the '{}' setting: {err}",
                    spec.key
                ))
            });
    }
    let mut settings = read_network_settings(path)?;
    settings.set(network, spec.key, value);
    crate::files::write_json(path, &settings)
}

/// Removes the stored value of one setting; `Ok(())` when nothing was stored.
pub async fn clear_network_setting(
    secrets: &dyn SecretStore,
    path: &Path,
    network: Network,
    spec: &NetworkSettingSpec,
) -> Result<(), EngineError> {
    let _guard = SETTINGS_LOCK.lock().await;
    clear_setting(secrets, path, network, spec).await
}

async fn clear_setting(
    secrets: &dyn SecretStore,
    path: &Path,
    network: Network,
    spec: &NetworkSettingSpec,
) -> Result<(), EngineError> {
    if spec.kind == NetworkSettingKind::Secret {
        return match secrets
            .delete_secret(&network_setting_secret(network, spec.key))
            .await
        {
            Ok(()) | Err(StorageError::SecretNotFound(_)) => Ok(()),
            Err(err) => Err(EngineError::internal_error(format!(
                "cannot delete the '{}' setting: {err}",
                spec.key
            ))),
        };
    }
    let mut settings = read_network_settings(path)?;
    if settings.get(network, spec.key).is_none() {
        return Ok(());
    }
    settings.remove(network, spec.key);
    crate::files::write_json(path, &settings)
}

/// The stored value of one setting: from the keyring for a secret setting, from the settings file
/// otherwise. `Ok(None)` means nothing is stored and the declared default applies.
pub async fn read_network_setting(
    secrets: &dyn SecretStore,
    path: &Path,
    network: Network,
    spec: &NetworkSettingSpec,
) -> Result<Option<String>, EngineError> {
    if spec.kind == NetworkSettingKind::Secret {
        return match secrets
            .get_secret(&network_setting_secret(network, spec.key))
            .await
        {
            Ok(value) => Ok(Some(value)),
            Err(StorageError::SecretNotFound(_)) => Ok(None),
            Err(err) => Err(EngineError::internal_error(format!(
                "cannot read the '{}' setting: {err}",
                spec.key
            ))),
        };
    }
    Ok(read_network_settings(path)?
        .get(network, spec.key)
        .map(str::to_string))
}

/// Environment variables a connector process receives, one per declared setting that names one.
///
/// The process always gets the value the app shows, so a default that is never stored is passed
/// too; an unset setting with no default is omitted. A secret setting never crosses this boundary.
pub fn network_setting_env(settings: &NetworkSettings, network: Network) -> Vec<(String, String)> {
    network_setting_specs(network)
        .iter()
        .filter(|spec| spec.kind != NetworkSettingKind::Secret)
        .filter_map(|spec| {
            let env = spec.env?;
            let value = effective_network_setting(settings, network, spec)?;
            (!value.is_empty()).then(|| (env.to_string(), value))
        })
        .collect()
}

/// Launch parameters for the Messenger sidecar, with the current settings as environment.
///
/// Only the settings that are set (or declared with a default) are passed: an unset proxy means
/// the process talks to Messenger directly.
pub fn messenger_sidecar_config(
    program: impl Into<PathBuf>,
    account_dir: impl Into<PathBuf>,
    settings: &NetworkSettings,
) -> SidecarConfig {
    let mut config = SidecarConfig::new(program, account_dir);
    config.env = network_setting_env(settings, Network::Messenger);
    config
}

/// Builds a connector for one network in one account's private directory.
#[async_trait::async_trait]
pub trait ConnectorFactory: Send + Sync {
    /// Networks this factory can offer, in the order `ListNetworks` reports them.
    ///
    /// Telegram is always present; `ready` and `setup` say whether a login can start now.
    async fn networks(&self) -> Vec<NetworkInfoDto>;

    /// Creates a connector whose own files may live in `account_dir` (created by the caller).
    async fn create(
        &self,
        network: Network,
        account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError>;
}

/// The production factory: in-process Telegram and, when installed, the Messenger sidecar.
///
/// It holds no credential of its own: the Telegram pair is read from `secrets` on every call, so
/// a pair entered while the app runs takes effect without a restart. Network settings are read the
/// same way, from `<data_dir>/network_settings.json` at the moment a connector is created.
#[derive(Clone)]
pub struct DefaultConnectorFactory {
    /// The platform secret store the Telegram application pair is sealed in.
    pub secrets: Arc<dyn SecretStore>,
    /// Executable speaking the connector protocol for Messenger; `None` omits Messenger.
    pub messenger_sidecar: Option<PathBuf>,
    /// Data directory holding [`NETWORK_SETTINGS_FILE`].
    pub data_dir: PathBuf,
}

impl DefaultConnectorFactory {
    pub fn new(
        secrets: Arc<dyn SecretStore>,
        messenger_sidecar: Option<PathBuf>,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            secrets,
            messenger_sidecar,
            data_dir: data_dir.into(),
        }
    }

    /// Reads the build's configuration: the Messenger sidecar from [`MESSENGER_SIDECAR_ENV`]. A
    /// missing sidecar only removes that network; it is not an error.
    pub fn from_env(secrets: Arc<dyn SecretStore>, data_dir: impl Into<PathBuf>) -> Self {
        let messenger_sidecar = std::env::var_os(MESSENGER_SIDECAR_ENV).map(PathBuf::from);
        Self {
            secrets,
            messenger_sidecar,
            data_dir: data_dir.into(),
        }
    }

    /// The non-secret settings currently on disk, for describing a network or launching it.
    pub fn network_settings(&self) -> NetworkSettings {
        load_network_settings(&network_settings_path(&self.data_dir))
    }

    /// Launch parameters for the Messenger sidecar with the settings in effect right now.
    ///
    /// `None` when this build has no sidecar. Read per call, so a saved setting applies to the next
    /// connector created; a running account keeps the ones it was launched with.
    pub fn messenger_config(&self, account_dir: impl Into<PathBuf>) -> Option<SidecarConfig> {
        let program = self.messenger_sidecar.clone()?;
        Some(messenger_sidecar_config(
            program,
            account_dir,
            &self.network_settings(),
        ))
    }

    /// The Telegram configuration in effect for this call, or the error a login gets when the user
    /// has not entered a pair yet.
    pub async fn telegram_config(&self) -> Result<TelegramConfig, ConnectorError> {
        resolve_telegram_app(self.secrets.as_ref())
            .await
            .map(|resolved| resolved.config)
            .ok_or_else(|| ConnectorError::NotSupported {
                feature: TELEGRAM_APP_FEATURE.to_string(),
            })
    }
}

#[async_trait::async_trait]
impl ConnectorFactory for DefaultConnectorFactory {
    async fn networks(&self) -> Vec<NetworkInfoDto> {
        let telegram = resolve_telegram_app(self.secrets.as_ref()).await;
        let mut networks = vec![convert::telegram_network_info(telegram.is_some())];
        if self.messenger_sidecar.is_some() {
            networks.push(convert::messenger_network_info(&self.network_settings()));
        }
        networks
    }

    async fn create(
        &self,
        network: Network,
        account_dir: PathBuf,
    ) -> Result<Arc<dyn Connector>, ConnectorError> {
        match network {
            Network::Telegram => {
                let config = self.telegram_config().await?;
                Ok(Arc::new(TelegramConnector::new(config)))
            }
            Network::Messenger => {
                let config = self.messenger_config(account_dir).ok_or_else(|| {
                    ConnectorError::NotSupported {
                        feature: convert::network_label(network).to_string(),
                    }
                })?;
                let connector = SidecarConnector::spawn(config).await?;
                Ok(Arc::new(connector))
            }
            Network::Fake => Err(ConnectorError::NotSupported {
                feature: convert::network_label(network).to_string(),
            }),
        }
    }
}

#[cfg(feature = "testing")]
pub use testing::{CapabilityOverrideConnector, FakeConnectorFactory, SealedTelegramFactory};

#[cfg(feature = "testing")]
mod testing {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, PoisonError};

    use alloy_connector::capabilities::Capabilities;
    use alloy_connector::fake::FakeConnector;
    use alloy_connector::{
        Connector, ConnectorError, ConnectorEvent, LoginInput, LoginStep, MediaRef,
        OutgoingMessage, Page, RemoteChat, RemoteMessage, RemoteUser, SendAck, SessionBlob,
    };
    use alloy_protocol::{Network, NetworkInfoDto};
    use alloy_storage::SecretStore;
    use async_trait::async_trait;
    use tokio::sync::mpsc::Sender;

    use super::{ConnectorFactory, DefaultConnectorFactory};
    use crate::convert::network_label;

    /// Adjusts the capabilities a fake connector reports before the engine sees them.
    pub type CapabilityOverride = Arc<dyn Fn(&mut Capabilities) + Send + Sync>;

    /// A factory that builds deterministic in-memory connectors for [`Network::Fake`].
    ///
    /// Tests keep a handle to every fake it created: the last one is the connector behind the
    /// current account, so a test can inject an incoming message without a network.
    #[derive(Default)]
    pub struct FakeConnectorFactory {
        created: Mutex<Vec<Arc<FakeConnector>>>,
        capability_override: Option<CapabilityOverride>,
    }

    impl FakeConnectorFactory {
        pub fn new() -> Self {
            Self::default()
        }

        /// Adjusts the capabilities every fake reports, so capability gates can be exercised on a
        /// network that really lacks a feature.
        pub fn with_capability_override(
            apply: impl Fn(&mut Capabilities) + Send + Sync + 'static,
        ) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                capability_override: Some(Arc::new(apply)),
            }
        }

        /// The connector created most recently, which is the one the engine is driving.
        pub fn last(&self) -> Option<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .last()
                .cloned()
        }

        /// Every connector created so far, oldest first.
        pub fn all(&self) -> Vec<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl ConnectorFactory for FakeConnectorFactory {
        async fn networks(&self) -> Vec<NetworkInfoDto> {
            vec![NetworkInfoDto {
                network: Network::Fake,
                ready: true,
                setup: None,
            }]
        }

        async fn create(
            &self,
            network: Network,
            _account_dir: PathBuf,
        ) -> Result<Arc<dyn Connector>, ConnectorError> {
            if network != Network::Fake {
                return Err(ConnectorError::NotSupported {
                    feature: network_label(network).to_string(),
                });
            }
            let fake = Arc::new(FakeConnector::new());
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fake.clone());
            match &self.capability_override {
                Some(apply) => {
                    let mut capabilities = fake.capabilities();
                    apply(&mut capabilities);
                    let connector: Arc<dyn Connector> = Arc::new(CapabilityOverrideConnector {
                        inner: fake,
                        capabilities,
                    });
                    Ok(connector)
                }
                None => {
                    let connector: Arc<dyn Connector> = fake;
                    Ok(connector)
                }
            }
        }
    }

    /// A factory that keeps the production Telegram credential gate and swaps the connector for
    /// the deterministic fake, so a test can prove a login reaches a connector without a network.
    ///
    /// `networks()` is exactly [`DefaultConnectorFactory`]'s, so readiness and the setup block
    /// under test are the production ones.
    pub struct SealedTelegramFactory {
        factory: DefaultConnectorFactory,
        created: Mutex<Vec<Arc<FakeConnector>>>,
    }

    impl SealedTelegramFactory {
        pub fn new(secrets: Arc<dyn SecretStore>, data_dir: impl Into<PathBuf>) -> Self {
            Self {
                factory: DefaultConnectorFactory::new(secrets, None, data_dir),
                created: Mutex::new(Vec::new()),
            }
        }

        /// The connector created most recently.
        pub fn last(&self) -> Option<Arc<FakeConnector>> {
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .last()
                .cloned()
        }
    }

    #[async_trait]
    impl ConnectorFactory for SealedTelegramFactory {
        async fn networks(&self) -> Vec<NetworkInfoDto> {
            self.factory.networks().await
        }

        async fn create(
            &self,
            network: Network,
            _account_dir: PathBuf,
        ) -> Result<Arc<dyn Connector>, ConnectorError> {
            if network != Network::Telegram {
                return Err(ConnectorError::NotSupported {
                    feature: network_label(network).to_string(),
                });
            }
            // The production gate: no pair sealed and none in the environment.
            self.factory.telegram_config().await?;
            let fake = Arc::new(FakeConnector::new());
            self.created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fake.clone());
            Ok(fake)
        }
    }

    /// Delegates every call to a real connector while reporting adjusted capabilities.
    ///
    /// Only for tests: it makes a capability observable as absent without a second connector
    /// implementation whose behaviour would drift from the fake's.
    pub struct CapabilityOverrideConnector {
        inner: Arc<FakeConnector>,
        capabilities: Capabilities,
    }

    #[async_trait]
    impl Connector for CapabilityOverrideConnector {
        fn network(&self) -> alloy_connector::Network {
            self.inner.network()
        }

        fn capabilities(&self) -> Capabilities {
            self.capabilities.clone()
        }

        async fn begin_login(&self) -> LoginStep {
            self.inner.begin_login().await
        }

        async fn submit_login(&self, input: LoginInput) -> LoginStep {
            self.inner.submit_login(input).await
        }

        async fn restore(&self, session: SessionBlob) -> Result<RemoteUser, ConnectorError> {
            self.inner.restore(session).await
        }

        async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError> {
            self.inner.start(events).await
        }

        async fn stop(&self) -> Result<(), ConnectorError> {
            self.inner.stop().await
        }

        async fn list_chats(
            &self,
            cursor: Option<String>,
            limit: usize,
        ) -> Result<Page<RemoteChat>, ConnectorError> {
            self.inner.list_chats(cursor, limit).await
        }

        async fn backfill(
            &self,
            remote_chat_id: &str,
            before: Option<String>,
            limit: usize,
        ) -> Result<Page<RemoteMessage>, ConnectorError> {
            self.inner.backfill(remote_chat_id, before, limit).await
        }

        async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError> {
            self.inner.send(message).await
        }

        async fn edit(
            &self,
            remote_chat_id: &str,
            remote_msg_id: &str,
            body: String,
            formatted: Option<String>,
        ) -> Result<(), ConnectorError> {
            self.inner
                .edit(remote_chat_id, remote_msg_id, body, formatted)
                .await
        }

        async fn delete(
            &self,
            remote_chat_id: &str,
            remote_msg_id: &str,
            for_everyone: bool,
        ) -> Result<(), ConnectorError> {
            self.inner
                .delete(remote_chat_id, remote_msg_id, for_everyone)
                .await
        }

        async fn react(
            &self,
            remote_chat_id: &str,
            remote_msg_id: &str,
            emoji: &str,
        ) -> Result<(), ConnectorError> {
            self.inner.react(remote_chat_id, remote_msg_id, emoji).await
        }

        async fn unreact(
            &self,
            remote_chat_id: &str,
            remote_msg_id: &str,
            emoji: &str,
        ) -> Result<(), ConnectorError> {
            self.inner
                .unreact(remote_chat_id, remote_msg_id, emoji)
                .await
        }

        async fn mark_read(
            &self,
            remote_chat_id: &str,
            up_to_remote_msg_id: &str,
        ) -> Result<(), ConnectorError> {
            self.inner
                .mark_read(remote_chat_id, up_to_remote_msg_id)
                .await
        }

        async fn set_typing(
            &self,
            remote_chat_id: &str,
            typing: bool,
        ) -> Result<(), ConnectorError> {
            self.inner.set_typing(remote_chat_id, typing).await
        }

        async fn fetch_media(
            &self,
            media: &MediaRef,
            dest_path: &Path,
        ) -> Result<u64, ConnectorError> {
            self.inner.fetch_media(media, dest_path).await
        }

        async fn logout(&self) -> Result<(), ConnectorError> {
            self.inner.logout().await
        }
    }
}
