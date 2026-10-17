//! Core AlloyClient and client session management.

use alloy_storage::SecretStore;
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

/// Lightweight Matrix client wrapper providing room list and timeline streams
/// via alloy-protocol DTOs.
pub struct AlloyClient {
    pub(crate) client: Client,
    pub(crate) secret_store: Box<dyn SecretStore>,
    /// Optional namespace isolating this account's secrets (`alloy:<ns>:...`).
    pub(crate) namespace: Option<String>,
    /// Live timelines by room, built lazily by [`AlloyClient::cached_timeline`].
    pub(crate) timelines: tokio::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<matrix_sdk_ui::timeline::Timeline>>,
    >,
    /// Background sliding-sync service, created lazily by [`AlloyClient::sync_service`].
    pub(crate) sync:
        tokio::sync::OnceCell<std::sync::Arc<matrix_sdk_ui::sync_service::SyncService>>,
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

        let state_store_path = data_dir.join("matrix");
        tokio::fs::create_dir_all(&state_store_path).await?;

        let secret_store = secret_store.unwrap_or_else(|| {
            Box::new(alloy_storage::InMemorySecretStore::new()) as Box<dyn SecretStore>
        });

        // Generate or retrieve passphrase for store encryption
        let namespace = namespace.map(str::to_owned);
        let store_passphrase_key = secret_key(namespace.as_deref(), "store:encryption_passphrase");
        let passphrase = match secret_store.get_secret(&store_passphrase_key).await {
            Ok(pp) => pp,
            Err(_) => {
                let pp = generate_passphrase();
                let _ = secret_store.set_secret(&store_passphrase_key, &pp).await;
                pp
            }
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
            timelines: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            sync: tokio::sync::OnceCell::new(),
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
            svc.stop().await;
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
