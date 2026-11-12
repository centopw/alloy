//! Telegram application pair as a runtime setting: readiness reporting, sealed storage and the
//! gate that turns a missing pair into an actionable `not_supported`.
//!
//! The connector itself is the deterministic fake (`SealedTelegramFactory` keeps the production
//! credential resolution), so nothing here touches the network.

use std::path::Path;
use std::sync::Arc;

use alloy_engine::factory::{SealedTelegramFactory, TELEGRAM_APP_FEATURE};
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    ApiHash, EngineCommand, EngineResponse, LoginStepDto, Network, NetworkInfoDto,
    NetworkSetupKind, TelegramAppSource,
};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::Store;
use tempfile::TempDir;

/// A valid pair: Telegram hashes are 32 hex characters.
const API_ID: i32 = 42;
const API_HASH: &str = "0123456789abcdef0123456789abcdef";
const KEY: [u8; 32] = [7; 32];

/// Opens the engine on `data_dir`, the way the app does on start.
async fn open_engine(data_dir: &Path, secrets: Arc<dyn SecretStore>) -> AppEngine {
    let factory = Arc::new(SealedTelegramFactory::new(
        secrets.clone(),
        data_dir.to_path_buf(),
    ));
    let ctx = EngineContext::open(data_dir.to_path_buf(), secrets, factory)
        .await
        .expect("open the engine");
    AppEngine::new(ctx)
}

/// An engine over a throwaway in-memory database, for the keyring cases that cannot reuse the
/// data directory (the database itself is sealed with a key that lives in the secret store).
fn engine_with_secrets(secrets: Arc<dyn SecretStore>) -> AppEngine {
    let store = Store::open_in_memory(KEY).expect("open the store");
    let dir = tempfile::tempdir().expect("temp dir");
    let factory = Arc::new(SealedTelegramFactory::new(
        secrets.clone(),
        dir.path().to_path_buf(),
    ));
    let ctx = EngineContext::with_store(store, dir.path().to_path_buf(), secrets, factory);
    AppEngine::new(ctx)
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

fn telegram(networks: &[NetworkInfoDto]) -> &NetworkInfoDto {
    networks
        .iter()
        .find(|info| info.network == Network::Telegram)
        .expect("Telegram is always listed")
}

async fn get_pair(engine: &AppEngine) -> EngineResponse {
    engine
        .handle(EngineCommand::GetTelegramApp)
        .await
        .expect("get the Telegram application pair")
}

#[tokio::test]
async fn telegram_is_listed_unready_with_setup_before_the_pair_is_entered() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    let networks = networks(&engine).await;
    let info = telegram(&networks);
    assert!(!info.ready);
    let setup = info.setup.as_ref().expect("a setup block");
    assert_eq!(setup.kind, NetworkSetupKind::TelegramApp);
    assert_eq!(setup.app_url, "https://my.telegram.org/apps");
    let keys: Vec<&str> = setup
        .credentials
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(keys, ["api_id", "api_hash"]);
    assert!(!setup.credentials[0].secret);
    assert!(setup.credentials[1].secret);
    assert!(setup.settings.is_empty());

    // No Messenger sidecar is configured, so Telegram is the only network.
    assert_eq!(networks.len(), 1);
}

#[tokio::test]
async fn begin_login_without_a_pair_is_an_actionable_not_supported() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    let error = engine
        .handle(EngineCommand::BeginLogin {
            network: Network::Telegram,
        })
        .await
        .expect_err("Telegram cannot log in without its application pair");

    assert_eq!(error.code, "not_supported");
    assert_eq!(error.reason.as_deref(), Some(TELEGRAM_APP_FEATURE));
    assert!(error.message.contains("my.telegram.org/apps"), "{error:?}");
    assert!(error.message.contains("Telegram application"), "{error:?}");
}

#[tokio::test]
async fn a_malformed_pair_is_invalid_argument() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    for (api_id, api_hash) in [
        (API_ID, ""),
        (API_ID, "not-hex"),
        (API_ID, "0123456789abcdef"),
        (API_ID, "0123456789abcdef0123456789abcdeZ"),
        (0, API_HASH),
        (-1, API_HASH),
    ] {
        let error = engine
            .handle(EngineCommand::SetTelegramApp {
                api_id,
                api_hash: ApiHash::new(api_hash),
            })
            .await
            .expect_err("a malformed pair is rejected");
        assert_eq!(error.code, "invalid_argument", "{api_id} {api_hash:?}");
        assert!(
            !error.message.contains(API_HASH),
            "the error must not echo a hash: {error:?}"
        );
    }

    // Nothing was stored by the rejected attempts.
    telegram_remains_unready(&engine).await;
}

async fn telegram_remains_unready(engine: &AppEngine) {
    assert!(!telegram(&networks(engine).await).ready);
    assert_eq!(
        get_pair(engine).await,
        EngineResponse::TelegramApp {
            configured: false,
            api_id: None,
            source: TelegramAppSource::None,
        }
    );
}

#[tokio::test]
async fn a_saved_pair_is_ready_immediately_and_never_echoes_the_hash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;

    let response = engine
        .handle(EngineCommand::SetTelegramApp {
            api_id: API_ID,
            api_hash: ApiHash::new(API_HASH),
        })
        .await
        .expect("store the pair");
    assert_eq!(
        response,
        EngineResponse::TelegramAppSet { configured: true }
    );

    // No restart: the next `ListNetworks` reads the sealed pair.
    let networks = networks(&engine).await;
    let info = telegram(&networks);
    assert!(info.ready);
    assert!(info.setup.is_none());

    let response = get_pair(&engine).await;
    assert_eq!(
        response,
        EngineResponse::TelegramApp {
            configured: true,
            api_id: Some(API_ID),
            source: TelegramAppSource::Runtime,
        }
    );
    assert!(
        !serde_json::to_string(&response)
            .expect("serialize the response")
            .contains(API_HASH),
        "the response must not carry the hash"
    );

    // The configured pair lets the login start and reach the connector.
    match engine
        .handle(EngineCommand::BeginLogin {
            network: Network::Telegram,
        })
        .await
        .expect("a configured pair starts the login")
    {
        EngineResponse::LoginStep { step, .. } => {
            assert_eq!(step, LoginStepDto::PhoneNumber);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[tokio::test]
async fn the_pair_survives_a_restart_because_it_is_sealed_not_stored_in_the_data_dir() {
    let dir: TempDir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    {
        let engine = open_engine(dir.path(), secrets.clone()).await;
        engine
            .handle(EngineCommand::SetTelegramApp {
                api_id: API_ID,
                api_hash: ApiHash::new(API_HASH),
            })
            .await
            .expect("store the pair");
    }

    // Nothing under the data directory carries the credential.
    assert!(
        !data_dir_mentions(dir.path(), API_HASH),
        "the pair must live in the secret store, not in the data directory"
    );

    let engine = open_engine(dir.path(), secrets.clone()).await;
    assert!(telegram(&networks(&engine).await).ready);
    assert_eq!(
        get_pair(&engine).await,
        EngineResponse::TelegramApp {
            configured: true,
            api_id: Some(API_ID),
            source: TelegramAppSource::Runtime,
        }
    );
}

#[tokio::test]
async fn a_different_secret_store_has_no_pair() {
    let dir = tempfile::tempdir().expect("temp dir");
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let engine = open_engine(dir.path(), secrets).await;
    engine
        .handle(EngineCommand::SetTelegramApp {
            api_id: API_ID,
            api_hash: ApiHash::new(API_HASH),
        })
        .await
        .expect("store the pair");

    // A fresh keyring (a new install, or the keyring entry was removed) knows nothing about it.
    let engine = engine_with_secrets(Arc::new(InMemorySecretStore::new()));
    telegram_remains_unready(&engine).await;
}

#[tokio::test]
async fn clearing_the_pair_makes_telegram_unready_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(dir.path(), Arc::new(InMemorySecretStore::new())).await;
    engine
        .handle(EngineCommand::SetTelegramApp {
            api_id: API_ID,
            api_hash: ApiHash::new(API_HASH),
        })
        .await
        .expect("store the pair");
    assert!(telegram(&networks(&engine).await).ready);

    let response = engine
        .handle(EngineCommand::ClearTelegramApp)
        .await
        .expect("clear the pair");
    assert_eq!(response, EngineResponse::TelegramAppCleared);

    telegram_remains_unready(&engine).await;

    let error = engine
        .handle(EngineCommand::BeginLogin {
            network: Network::Telegram,
        })
        .await
        .expect_err("the login cannot start again");
    assert_eq!(error.code, "not_supported");
    assert_eq!(error.reason.as_deref(), Some(TELEGRAM_APP_FEATURE));
}

/// Whether any file under `root` contains `needle` as text.
fn data_dir_mentions(root: &Path, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if data_dir_mentions(&path, needle) {
                return true;
            }
        } else if let Ok(bytes) = std::fs::read(&path) {
            if String::from_utf8_lossy(&bytes).contains(needle) {
                return true;
            }
        }
    }
    false
}
