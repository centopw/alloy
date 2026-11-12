//! Runtime network settings: what each network declares, validation, persistence and delivery.
//!
//! Messenger is the first network with settings of its own (proxy and platform). The factory is
//! built with a sidecar path that need not exist: `ListNetworks` and `messenger_config` never
//! spawn it, and a real sidecar is not part of what these tests prove.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alloy_engine::factory::{
    self, ConnectorFactory, DefaultConnectorFactory, NetworkSettingSpec, NetworkSettingValidation,
};
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineResponse, Network, NetworkInfoDto, NetworkSettingDto, NetworkSettingKind,
    NetworkSetupDto, NetworkSetupKind,
};
use alloy_storage::{InMemorySecretStore, SecretStore};
use tempfile::TempDir;

/// A path that is never executed; presence is all `ListNetworks` checks.
const SIDECAR: &str = "/nonexistent/alloy-meta";
const PROXY: &str = "socks5://127.0.0.1:1080";

async fn open_engine(data_dir: &Path, secrets: Arc<dyn SecretStore>) -> AppEngine {
    let factory: Arc<dyn ConnectorFactory> = Arc::new(DefaultConnectorFactory::new(
        secrets.clone(),
        Some(sidecar()),
        data_dir.to_path_buf(),
    ));
    let ctx = EngineContext::open(data_dir.to_path_buf(), secrets, factory)
        .await
        .expect("open the engine");
    AppEngine::new(ctx)
}

fn sidecar() -> PathBuf {
    PathBuf::from(SIDECAR)
}

fn default_factory(data_dir: &Path, secrets: Arc<dyn SecretStore>) -> DefaultConnectorFactory {
    DefaultConnectorFactory::new(secrets, Some(sidecar()), data_dir.to_path_buf())
}

async fn networks(engine: &AppEngine) -> Vec<NetworkInfoDto> {
    match engine
        .handle(EngineCommand::ListNetworks)
        .await
        .expect("list networks")
    {
        EngineResponse::Networks { networks } => networks,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn messenger(networks: &[NetworkInfoDto]) -> &NetworkInfoDto {
    networks
        .iter()
        .find(|info| info.network == Network::Messenger)
        .expect("the sidecar is configured, so Messenger is listed")
}

fn setup_of(info: &NetworkInfoDto) -> &NetworkSetupDto {
    info.setup.as_ref().expect("a setup block")
}

fn setting<'a>(setup: &'a NetworkSetupDto, key: &str) -> &'a NetworkSettingDto {
    setup
        .settings
        .iter()
        .find(|setting| setting.key == key)
        .unwrap_or_else(|| panic!("the setup declares '{key}'"))
}

async fn set(
    engine: &AppEngine,
    key: &str,
    value: &str,
) -> Result<EngineResponse, alloy_engine::EngineError> {
    engine
        .handle(EngineCommand::SetNetworkSetting {
            network: Network::Messenger,
            key: key.to_string(),
            value: value.to_string(),
        })
        .await
}

async fn clear(engine: &AppEngine, key: &str) -> Result<EngineResponse, alloy_engine::EngineError> {
    engine
        .handle(EngineCommand::ClearNetworkSetting {
            network: Network::Messenger,
            key: key.to_string(),
        })
        .await
}

/// Text of a file, empty when it does not exist.
fn file_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[tokio::test]
async fn messenger_declares_proxy_and_platform_with_their_defaults() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;
    let networks = networks(&engine).await;

    let info = messenger(&networks);
    // Settings never make a network unready: the sidecar exists, so a login can start.
    assert!(info.ready);
    let setup = setup_of(info);
    assert_eq!(setup.kind, NetworkSetupKind::Messenger);
    assert_eq!(setup.app_url, "");
    assert!(setup.credentials.is_empty());

    let keys: Vec<&str> = setup
        .settings
        .iter()
        .map(|setting| setting.key.as_str())
        .collect();
    assert_eq!(keys, ["proxy", "platform"]);

    let proxy = setting(setup, "proxy");
    assert_eq!(proxy.kind, NetworkSettingKind::Text);
    assert_eq!(proxy.label, "Proxy URL (http://, https://, socks5://)");
    assert!(proxy.options.is_empty());
    assert_eq!(proxy.value, None, "an unset proxy has no value");

    let platform = setting(setup, "platform");
    assert_eq!(platform.kind, NetworkSettingKind::Choice);
    assert_eq!(platform.value.as_deref(), Some("android"));
    let options: Vec<(&str, &str)> = platform
        .options
        .iter()
        .map(|option| (option.value.as_str(), option.label.as_str()))
        .collect();
    assert_eq!(
        options,
        [
            ("android", "Messenger for Android"),
            ("ios", "Messenger for iOS"),
        ]
    );

    // Telegram still declares credentials only: its hash is never a setting, and no setting value
    // stands in for it.
    let telegram = networks
        .iter()
        .find(|info| info.network == Network::Telegram)
        .expect("Telegram is always listed");
    let setup = setup_of(telegram);
    assert_eq!(setup.kind, NetworkSetupKind::TelegramApp);
    assert!(setup.settings.is_empty());
    assert!(
        setup
            .settings
            .iter()
            .all(|setting| setting.key != "api_hash"),
        "the api_hash is a credential, never a setting"
    );
}

#[tokio::test]
async fn a_proxy_that_is_not_a_supported_url_is_rejected_and_a_valid_one_is_persisted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    // An empty value clears rather than failing, so it is not in this list.
    for bad in [
        "127.0.0.1:1080",
        "ftp://127.0.0.1:1080",
        "http://",
        "http://:1080",
        "socks5://",
        "socks4://127.0.0.1:1080",
    ] {
        let error = set(&engine, "proxy", bad)
            .await
            .expect_err("a proxy that is not http/https/socks5 with a host is rejected");
        assert_eq!(error.code, "invalid_argument", "{bad}");
        assert!(error.message.contains("proxy"), "{error:?}");
    }
    // Nothing was stored by the rejected attempts.
    let listed = networks(&engine).await;
    assert_eq!(setting(setup_of(messenger(&listed)), "proxy").value, None);

    let response = set(&engine, "proxy", PROXY).await.expect("store the proxy");
    match &response {
        EngineResponse::NetworkSettingSet { setting } => {
            assert_eq!(setting.key, "proxy");
            assert_eq!(setting.value.as_deref(), Some(PROXY));
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let listed = networks(&engine).await;
    assert_eq!(
        setting(setup_of(messenger(&listed)), "proxy")
            .value
            .as_deref(),
        Some(PROXY)
    );
    let persisted = file_text(&factory::network_settings_path(dir.path()));
    assert!(persisted.contains(PROXY), "{persisted}");
    assert!(persisted.contains("messenger"), "{persisted}");
}

#[tokio::test]
async fn the_platform_choice_is_validated_and_persisted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    let error = set(&engine, "platform", "windows")
        .await
        .expect_err("only the declared options are accepted");
    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("platform"), "{error:?}");
    assert!(error.message.contains("android"), "{error:?}");
    assert!(error.message.contains("ios"), "{error:?}");

    set(&engine, "platform", "ios")
        .await
        .expect("store the platform");
    let listed = networks(&engine).await;
    assert_eq!(
        setting(setup_of(messenger(&listed)), "platform")
            .value
            .as_deref(),
        Some("ios")
    );
}

#[tokio::test]
async fn clearing_a_setting_restores_the_default_and_unknown_keys_are_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    set(&engine, "proxy", PROXY).await.expect("store the proxy");
    set(&engine, "platform", "ios")
        .await
        .expect("store the platform");

    let response = clear(&engine, "proxy").await.expect("clear the proxy");
    assert_eq!(response, EngineResponse::NetworkSettingCleared);
    let listed = networks(&engine).await;
    let setup = setup_of(messenger(&listed));
    assert_eq!(setting(setup, "proxy").value, None);
    assert!(
        !file_text(&factory::network_settings_path(dir.path())).contains(PROXY),
        "the cleared value must not stay in the file"
    );

    clear(&engine, "platform")
        .await
        .expect("clear the platform");
    let listed = networks(&engine).await;
    assert_eq!(
        setting(setup_of(messenger(&listed)), "platform")
            .value
            .as_deref(),
        Some("android"),
        "clearing restores the declared default"
    );

    // Clearing again and clearing an unknown key behave predictably.
    assert_eq!(
        clear(&engine, "platform")
            .await
            .expect("clearing twice is fine"),
        EngineResponse::NetworkSettingCleared
    );
    let error = clear(&engine, "endpoint").await.expect_err("unknown key");
    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("endpoint"), "{error:?}");
    assert!(error.message.contains("proxy"), "{error:?}");
}

#[tokio::test]
async fn a_setting_survives_a_restart() {
    let dir: TempDir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    {
        let engine = open_engine(dir.path(), secrets.clone()).await;
        set(&engine, "proxy", PROXY).await.expect("store the proxy");
        set(&engine, "platform", "ios")
            .await
            .expect("store the platform");
    }

    let engine = open_engine(dir.path(), secrets.clone()).await;
    let listed = networks(&engine).await;
    let setup = setup_of(messenger(&listed));
    assert_eq!(setting(setup, "proxy").value.as_deref(), Some(PROXY));
    assert_eq!(setting(setup, "platform").value.as_deref(), Some("ios"));
}

#[tokio::test]
async fn a_secret_setting_round_trips_through_the_secret_store_and_never_reaches_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let path = factory::network_settings_path(dir.path());

    // No shipped network declares a secret setting yet, so this drives the mechanism with one:
    // the same functions `SetNetworkSetting` uses for a declared secret.
    let spec = NetworkSettingSpec {
        key: "test_secret",
        label: "Test secret",
        kind: NetworkSettingKind::Secret,
        options: &[],
        default: None,
        validation: NetworkSettingValidation::Free,
        env: None,
    };
    const VALUE: &str = "sealed-value-1234";

    factory::store_network_setting(secrets.as_ref(), &path, Network::Messenger, &spec, VALUE)
        .await
        .expect("store the secret");
    assert_eq!(
        factory::read_network_setting(secrets.as_ref(), &path, Network::Messenger, &spec)
            .await
            .expect("read the secret")
            .as_deref(),
        Some(VALUE)
    );
    assert_eq!(
        secrets
            .get_secret(&factory::network_setting_secret(
                Network::Messenger,
                "test_secret"
            ))
            .await
            .expect("the secret is sealed under its own key"),
        VALUE
    );
    let persisted = file_text(&path);
    assert!(!persisted.contains(VALUE), "{persisted}");
    assert!(!persisted.contains("test_secret"), "{persisted}");

    factory::clear_network_setting(secrets.as_ref(), &path, Network::Messenger, &spec)
        .await
        .expect("clear the secret");
    assert_eq!(
        factory::read_network_setting(secrets.as_ref(), &path, Network::Messenger, &spec)
            .await
            .expect("read the cleared secret"),
        None
    );
}

#[tokio::test]
async fn the_next_sidecar_launch_carries_the_settings_in_effect() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let launcher = default_factory(dir.path(), secrets.clone());
    let account_dir = dir.path().join("accounts/one");

    let config = launcher
        .messenger_config(&account_dir)
        .expect("the sidecar is configured");
    assert!(
        !config
            .env
            .iter()
            .any(|(key, _)| key == factory::MESSENGER_PROXY_ENV),
        "an unset proxy is omitted: {:?}",
        config.env
    );
    assert_eq!(
        config.env,
        [(
            factory::MESSENGER_PLATFORM_ENV.to_string(),
            "android".to_string()
        )],
        "the declared default is passed, so the sidecar never guesses"
    );

    // The same factory instance, no restart: the change applies to the next launch.
    let engine = open_engine(dir.path(), secrets.clone()).await;
    set(&engine, "proxy", PROXY).await.expect("store the proxy");
    set(&engine, "platform", "ios")
        .await
        .expect("store the platform");

    let config = launcher
        .messenger_config(&account_dir)
        .expect("the sidecar is still configured");
    assert_eq!(
        config.env,
        [
            (factory::MESSENGER_PROXY_ENV.to_string(), PROXY.to_string()),
            (
                factory::MESSENGER_PLATFORM_ENV.to_string(),
                "ios".to_string()
            ),
        ]
    );
}
