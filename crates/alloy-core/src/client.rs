//! Core AlloyClient and client session management.

use alloy_storage::{SecretStore, StorageError};
use base64::Engine;
use matrix_sdk::Client;
use matrix_sdk_sqlite::SqliteStoreConfig;
use std::path::Path;
use url::Url;

use crate::{AlloyError, AlloyResult};

/// Generate a random passphrase for store encryption
fn generate_passphrase() -> String {
    let random_bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(random_bytes)
}

/// Build a secret-store key, optionally namespaced per account.
pub(crate) fn secret_key(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) => format!("alloy:{ns}:{name}"),
        None => format!("alloy:{name}"),
    }
}

/// Hosts allowed to use plain `http`: loopback and the Android emulator's host alias.
/// `Url::host_str` keeps the brackets around IPv6 literals.
const CLEARTEXT_HOSTS: [&str; 4] = ["localhost", "127.0.0.1", "[::1]", "10.0.2.2"];

/// Accept `https` homeservers, and `http` only for local development hosts. Access tokens and
/// passwords must never travel in cleartext to a remote server.
pub(crate) fn require_secure_homeserver(url: &Url) -> AlloyResult<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" => match url.host_str() {
            Some(host) if CLEARTEXT_HOSTS.contains(&host) => Ok(()),
            _ => Err(AlloyError::InsecureHomeserverUrl),
        },
        _ => Err(AlloyError::InvalidHomeserverUrl),
    }
}

/// Every secret an account stores. Keyring backends cannot enumerate keys, so they are named here.
const ACCOUNT_SECRET_NAMES: [&str; 2] = ["session", "store:encryption_passphrase"];

/// Delete every secret stored for one account (see [`secret_key`]).
///
/// Deleting an absent secret succeeds, so this is safe for accounts whose login never completed.
pub async fn delete_account_secrets(
    store: &dyn SecretStore,
    namespace: Option<&str>,
) -> AlloyResult<()> {
    for name in ACCOUNT_SECRET_NAMES {
        store.delete_secret(&secret_key(namespace, name)).await?;
    }
    Ok(())
}

/// Lightweight Matrix client wrapper providing room list and timeline streams
/// via alloy-protocol DTOs.
pub struct AlloyClient {
    pub(crate) client: Client,
    pub(crate) secret_store: Box<dyn SecretStore>,
    /// Optional namespace isolating this account's secrets (`alloy:<ns>:...`).
    pub(crate) namespace: Option<String>,
    /// Latest incoming verification request awaiting adoption.
    pub(crate) pending_incoming: std::sync::Arc<
        tokio::sync::Mutex<Option<matrix_sdk::encryption::verification::VerificationRequest>>,
    >,
    /// The verification currently in progress (one at a time), see `crypto.rs`.
    pub(crate) verification: tokio::sync::Mutex<crate::crypto::ActiveVerification>,
    /// Live timelines by room, built lazily by [`AlloyClient::cached_timeline`].
    pub(crate) timelines: tokio::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<matrix_sdk_ui::timeline::Timeline>>,
    >,
    /// Background sliding-sync service, created lazily by [`AlloyClient::sync_service`].
    pub(crate) sync:
        tokio::sync::OnceCell<std::sync::Arc<matrix_sdk_ui::sync_service::SyncService>>,
    /// Space hierarchy graph, created lazily by `AlloyClient::space_parents`.
    pub(crate) spaces: tokio::sync::OnceCell<std::sync::Arc<matrix_sdk_ui::spaces::SpaceService>>,
    /// Direct-message room id per target user (`user_id` -> `room_id`), kept for the
    /// session so repeated `start_direct_message` calls never open a second room.
    pub(crate) direct_rooms: tokio::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl AlloyClient {
    /// Create a new AlloyClient with SQLite store encrypted via passphrase in SecretStore.
    pub async fn new(
        homeserver_url: &str,
        data_dir: &Path,
        secret_store: Option<Box<dyn SecretStore>>,
    ) -> AlloyResult<Self> {
        Self::open(homeserver_url, data_dir, secret_store, None).await
    }

    /// Like [`AlloyClient::new`] but isolates secrets under `namespace`
    /// (one namespace per account when several accounts share a secret store).
    pub async fn open(
        homeserver_url: &str,
        data_dir: &Path,
        secret_store: Option<Box<dyn SecretStore>>,
        namespace: Option<&str>,
    ) -> AlloyResult<Self> {
        let homeserver =
            Url::parse(homeserver_url).map_err(|_| AlloyError::InvalidHomeserverUrl)?;
        require_secure_homeserver(&homeserver)?;

        let state_store_path = data_dir.join("matrix");
        tokio::fs::create_dir_all(&state_store_path).await?;

        let secret_store = secret_store.unwrap_or_else(|| {
            Box::new(alloy_storage::InMemorySecretStore::new()) as Box<dyn SecretStore>
        });

        // Generate the store passphrase on first use only. Any other lookup failure (keychain locked,
        // backend unavailable) must not replace it: the SQLite store encrypted with the old passphrase
        // would become unreadable and its data silently lost.
        let namespace = namespace.map(str::to_owned);
        let store_passphrase_key = secret_key(namespace.as_deref(), "store:encryption_passphrase");
        let passphrase = match secret_store.get_secret(&store_passphrase_key).await {
            Ok(pp) => pp,
            Err(StorageError::SecretNotFound(_)) => {
                let pp = generate_passphrase();
                secret_store.set_secret(&store_passphrase_key, &pp).await?;
                pp
            }
            Err(e) => return Err(e.into()),
        };

        // Configure SQLite store with encryption
        let store_config = SqliteStoreConfig::new(&state_store_path).passphrase(Some(&passphrase));

        let client = Client::builder()
            .homeserver_url(homeserver)
            .sqlite_store_with_config_and_cache_path(store_config, None::<&Path>)
            .build()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(Self {
            client,
            secret_store,
            namespace,
            pending_incoming: Default::default(),
            verification: tokio::sync::Mutex::new(Default::default()),
            timelines: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            sync: tokio::sync::OnceCell::new(),
            spaces: tokio::sync::OnceCell::new(),
            direct_rooms: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Restore a previously logged-in session from the store.
    pub async fn restore_session(&self) -> AlloyResult<bool> {
        if self.client.user_id().is_some() {
            return Ok(true);
        }
        match self.secret_store.get_secret(&self.session_key()).await {
            Ok(json) => {
                let session: matrix_sdk::authentication::matrix::MatrixSession =
                    serde_json::from_str(&json).map_err(|e| AlloyError::Auth(e.to_string()))?;
                self.client
                    .restore_session(session)
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    pub(crate) fn session_key(&self) -> String {
        secret_key(self.namespace.as_deref(), "session")
    }

    /// Delete every secret this account stores from the secret store.
    pub(crate) async fn delete_local_secrets(&self) -> AlloyResult<()> {
        delete_account_secrets(self.secret_store.as_ref(), self.namespace.as_deref()).await
    }

    /// Start (once) the background sync service and return it. Requires a logged-in client.
    pub async fn sync_service(
        &self,
    ) -> AlloyResult<std::sync::Arc<matrix_sdk_ui::sync_service::SyncService>> {
        if self.client.user_id().is_none() {
            return Err(AlloyError::NotLoggedIn);
        }
        let svc = self
            .sync
            .get_or_try_init(|| async {
                let svc = matrix_sdk_ui::sync_service::SyncService::builder(self.client.clone())
                    .build()
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
                svc.start().await;
                Ok::<_, AlloyError>(std::sync::Arc::new(svc))
            })
            .await?;
        Ok(svc.clone())
    }

    /// Stop background sync if it was started.
    pub async fn stop_sync(&self) {
        self.timelines.lock().await.clear();
        if let Some(svc) = self.sync.get() {
            // `SyncService::stop` waits for in-flight long-polls and can take very long, so
            // stop in the background and give it a short grace period only.
            let svc = svc.clone();
            let stopper = tokio::spawn(async move { svc.stop().await });
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), stopper).await;
        }
    }

    /// Get the matrix-sdk Client for advanced operations.
    pub fn inner_client(&self) -> &Client {
        &self.client
    }

    /// Sync once.
    pub async fn sync_once(&self) -> AlloyResult<()> {
        let settings =
            matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::from_secs(2));
        self.client
            .sync_once(settings)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Get user ID if logged in.
    pub fn user_id(&self) -> Option<&str> {
        self.client.user_id().map(|uid| uid.as_str())
    }
}
