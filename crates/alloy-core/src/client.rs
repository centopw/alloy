//! Core AlloyClient and client session management.

use alloy_storage::{SecretStore, StorageError};
use base64::Engine;
use matrix_sdk::Client;
use matrix_sdk_sqlite::SqliteStoreConfig;
use std::path::Path;
use std::sync::Arc;
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

/// A session as it is stored in the secret store.
///
/// The format is tagged explicitly: an OAuth session's JSON also looks like a password session,
/// and restoring it as one would drop its client id, refresh support and revocation path.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "format", content = "session", rename_all = "snake_case")]
enum StoredSession {
    Matrix(matrix_sdk::authentication::matrix::MatrixSession),
    Oauth(StoredOAuthSession),
}

/// An OAuth session in a storable shape: the SDK's `OAuthSession` is not serializable, so the
/// client id is stored next to the user session and put back on restore.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredOAuthSession {
    client_id: String,
    user: matrix_sdk::authentication::oauth::UserSession,
}

impl StoredOAuthSession {
    fn into_session(self) -> matrix_sdk::authentication::oauth::OAuthSession {
        matrix_sdk::authentication::oauth::OAuthSession {
            client_id: oauth2::ClientId::new(self.client_id),
            user: self.user,
        }
    }
}

/// Reads a stored session. Sessions written before 0.2.9 are a bare password session with no tag.
fn parse_stored_session(json: &str) -> AlloyResult<StoredSession> {
    if let Ok(session) =
        serde_json::from_str::<matrix_sdk::authentication::matrix::MatrixSession>(json)
    {
        return Ok(StoredSession::Matrix(session));
    }
    serde_json::from_str(json)
        .map_err(|e| AlloyError::Auth(format!("stored session is unreadable: {e}")))
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
    /// Push-rule state shared by all notification calls, created on first use. Its sync
    /// handler keeps the rules current and writes update them directly, so it must live
    /// as long as the client.
    pub(crate) notification_settings:
        tokio::sync::OnceCell<matrix_sdk::notification_settings::NotificationSettings>,
    /// Newest `updated_at` of the synced preferences this client wrote or reported (0 when
    /// none), so the server echo of its own write is not reported back as a remote change.
    pub(crate) prefs_updated_at: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Recently fetched link previews, so the same link is not re-requested while it is still fresh.
    pub(crate) url_previews: crate::url_preview::PreviewCache,
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

        let http =
            crate::tls::matrix_http_client().map_err(|e| AlloyError::HttpClient(e.to_string()))?;
        let client = Client::builder()
            .http_client(http)
            .homeserver_url(homeserver)
            .handle_refresh_tokens()
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
            notification_settings: tokio::sync::OnceCell::new(),
            prefs_updated_at: Default::default(),
            url_previews: Default::default(),
        })
    }

    /// Restore a previously logged-in session from the store.
    pub async fn restore_session(&self) -> AlloyResult<bool> {
        if self.client.user_id().is_some() {
            return Ok(true);
        }
        let json = match self.secret_store.get_secret(&self.session_key()).await {
            Ok(json) => json,
            Err(_) => return Ok(false),
        };
        match parse_stored_session(&json)? {
            StoredSession::Matrix(session) => self
                .client
                .restore_session(session)
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?,
            StoredSession::Oauth(session) => self
                .client
                .oauth()
                .restore_session(
                    session.into_session(),
                    matrix_sdk::store::RoomLoadSettings::default(),
                )
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?,
        }
        Ok(true)
    }

    /// Stores a password session, so the next start restores it.
    pub(crate) async fn save_matrix_session(
        &self,
        session: matrix_sdk::authentication::matrix::MatrixSession,
    ) -> AlloyResult<()> {
        self.store_session(&StoredSession::Matrix(session)).await
    }

    /// Stores the current OAuth session, so restored tokens and refreshed tokens survive a restart.
    /// A password session has nothing OAuth to store and is left untouched.
    pub async fn persist_oauth_session(&self) -> AlloyResult<()> {
        let Some(session) = self.client.oauth().full_session() else {
            return Ok(());
        };
        let stored = StoredOAuthSession {
            client_id: session.client_id.to_string(),
            user: session.user,
        };
        self.store_session(&StoredSession::Oauth(stored)).await
    }

    /// Whether the current session authenticates through OAuth 2.0 (MAS).
    ///
    /// `user_session()` is not a usable check here: it is populated for password sessions too.
    /// Only an OAuth session has the client registration data that `full_session()` requires.
    pub fn is_oauth_session(&self) -> bool {
        self.client.oauth().full_session().is_some()
    }

    /// Reports token refreshes and invalidated tokens for the current session.
    pub fn session_changes(&self) -> tokio::sync::broadcast::Receiver<matrix_sdk::SessionChange> {
        self.client.subscribe_to_session_changes()
    }

    /// Writes the OAuth session back to the secret store on every token refresh.
    ///
    /// Run it as a task for as long as the session is active; abort it on logout. A password
    /// session never refreshes, so the task just waits for the client to go away.
    pub async fn watch_session_tokens(self: Arc<Self>) {
        let mut changes = self.session_changes();
        loop {
            match changes.recv().await {
                Ok(matrix_sdk::SessionChange::TokensRefreshed) => {
                    if let Err(e) = self.persist_oauth_session().await {
                        tracing::warn!(error = %e, "could not persist refreshed tokens");
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    async fn store_session(&self, session: &StoredSession) -> AlloyResult<()> {
        let json = serde_json::to_string(session).map_err(|e| AlloyError::Auth(e.to_string()))?;
        self.secret_store
            .set_secret(&self.session_key(), &json)
            .await?;
        Ok(())
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
                // Offline mode: a failing sync no longer ends the service; it checks the server and resumes by
                // itself. Without it the service stayed stopped after one error until the app was restarted.
                let svc = matrix_sdk_ui::sync_service::SyncService::builder(self.client.clone())
                    .with_offline_mode()
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

    /// Restarts sync and drops the cached timelines, so the next time a room opens it is rebuilt from what the
    /// server has. This is what the manual Refresh runs: a sync that hung on a dead connection looks "running".
    /// The restart happens in the background because stopping waits for the in-flight request.
    pub async fn refresh_sync(&self) -> AlloyResult<()> {
        let svc = self.sync_service().await?;
        self.timelines.lock().await.clear();
        tokio::spawn(async move {
            svc.stop().await;
            svc.start().await;
        });
        Ok(())
    }

    /// Starts the sync service again whenever it ends in an error, pausing longer after each repeated failure.
    /// Runs until the returned task is aborted.
    pub async fn supervise_sync(&self) -> AlloyResult<tokio::task::JoinHandle<()>> {
        use matrix_sdk_ui::sync_service::State;
        let svc = self.sync_service().await?;
        Ok(tokio::spawn(async move {
            let mut states = svc.state();
            let mut failures: u32 = 0;
            while let Some(state) = states.next().await {
                match state {
                    State::Running => failures = 0,
                    State::Error(error) => {
                        failures = failures.saturating_add(1);
                        let pause =
                            std::time::Duration::from_secs((1_u64 << failures.min(5)).min(30));
                        tracing::warn!(%error, ?pause, "sync stopped with an error, restarting");
                        tokio::time::sleep(pause).await;
                        svc.start().await;
                    }
                    _ => {}
                }
            }
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str =
        r#"{"user_id":"@alice:example.org","device_id":"DEVICE","access_token":"token"}"#;
    const MATRIX: &str = r#"{"format":"matrix","session":{"user_id":"@alice:example.org","device_id":"DEVICE","access_token":"token"}}"#;
    const OAUTH: &str = r#"{"format":"oauth","session":{"client_id":"client","user":{"user_id":"@alice:example.org","device_id":"DEVICE","access_token":"token","refresh_token":"refresh"}}}"#;

    #[test]
    fn a_session_stored_before_0_2_9_is_read_as_a_password_session() {
        assert!(matches!(
            parse_stored_session(LEGACY).unwrap(),
            StoredSession::Matrix(_)
        ));
    }

    #[test]
    fn tagged_sessions_keep_their_kind() {
        assert!(matches!(
            parse_stored_session(MATRIX).unwrap(),
            StoredSession::Matrix(_)
        ));
        // An OAuth session must not be restored as a password session: its client id and refresh
        // support would be dropped, and token refresh would silently stop working.
        match parse_stored_session(OAUTH).unwrap() {
            StoredSession::Oauth(session) => {
                assert_eq!(session.client_id, "client");
                assert_eq!(session.user.meta.user_id.as_str(), "@alice:example.org");
                assert_eq!(
                    session.user.tokens.refresh_token.as_deref(),
                    Some("refresh")
                );
            }
            StoredSession::Matrix(_) => panic!("an OAuth session parsed as a password session"),
        }
    }

    #[test]
    fn an_unreadable_stored_session_is_reported() {
        assert!(matches!(
            parse_stored_session("not json"),
            Err(AlloyError::Auth(_))
        ));
        assert!(matches!(
            parse_stored_session(r#"{"format":"oauth","session":{"client_id":1}}"#),
            Err(AlloyError::Auth(_))
        ));
    }
}
