//! Shared engine context passed to command handlers and background actors.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use alloy_storage::{SecretStore, StorageError};
use alloy_store::Store;
use base64::Engine as _;

use crate::accounts::AccountManager;
use crate::error::{EngineError, map_store};
use crate::factory::ConnectorFactory;
use crate::login::LoginManager;
use crate::projection::{ChangeBus, TimelineRegistry};
use crate::registry::SubscriptionRegistry;
use crate::sink::EventSink;

/// Secret under which the store's sealing key is kept in the platform secret store.
pub const STORE_KEY_SECRET: &str = "alloy.store.key.v1";

const STORE_KEY_LEN: usize = 32;

/// Files and directories `SendAttachment` may read. Paths are canonical.
#[derive(Default)]
struct AttachmentAllowlist {
    files: HashSet<PathBuf>,
    roots: Vec<PathBuf>,
}

/// Everything a command handler or account actor needs.
///
/// Cloning shares the store, the subscriptions and the running accounts; the sink and bus are
/// themselves cloneable handles.
#[derive(Clone)]
pub struct EngineContext {
    pub sink: EventSink,
    pub data_dir: PathBuf,
    pub secrets: Arc<dyn SecretStore>,
    pub subscriptions: SubscriptionRegistry,
    /// Start account actors on [`crate::AppEngine::start`] (disable for headless use).
    pub background: bool,
    /// The local database: accounts, chats, messages, media rows, outbox and change feed.
    pub store: Store,
    /// One actor and one outbox worker per connected account.
    pub accounts: AccountManager,
    /// Login attempts waiting for the user's next answer.
    pub logins: LoginManager,
    /// Builds the connector for a network this build can reach.
    pub factory: Arc<dyn ConnectorFactory>,
    /// Store-change fan-out feeding the UI projections.
    pub bus: ChangeBus,
    /// Open timeline windows, one per subscribed chat.
    pub timelines: TimelineRegistry,
    attachments: Arc<Mutex<AttachmentAllowlist>>,
    unrestricted_attachments: bool,
}

impl EngineContext {
    /// Opens the on-device database under `data_dir`, creating it on first start.
    ///
    /// The sealing key for connector sessions is generated once and kept in the platform secret
    /// store, so a restart can decrypt the sessions the previous run stored.
    pub async fn open(
        data_dir: PathBuf,
        secrets: Arc<dyn SecretStore>,
        factory: Arc<dyn ConnectorFactory>,
    ) -> Result<Self, EngineError> {
        std::fs::create_dir_all(&data_dir).map_err(|err| {
            EngineError::internal_error(format!(
                "cannot create the data directory {}: {err}",
                data_dir.display()
            ))
        })?;
        let key = store_key(secrets.as_ref()).await?;
        let store = Store::open(data_dir.join("alloy.db"), key).map_err(map_store)?;
        let context = Self::with_store(store, data_dir, secrets, factory);
        // No connector is running yet: a directory with no stored account belongs to a login
        // attempt that died before finishing, and nothing will ever use it.
        context.remove_orphan_account_dirs().await;
        Ok(context)
    }

    /// Builds a context around an already open store; the entry point for tests that keep an
    /// in-memory database or reopen the same file.
    pub fn with_store(
        store: Store,
        data_dir: PathBuf,
        secrets: Arc<dyn SecretStore>,
        factory: Arc<dyn ConnectorFactory>,
    ) -> Self {
        let sink = EventSink::new();
        Self {
            sink,
            data_dir,
            secrets,
            subscriptions: SubscriptionRegistry::new(),
            background: true,
            store,
            accounts: AccountManager::new(),
            logins: LoginManager::new(),
            factory,
            bus: ChangeBus::new(),
            timelines: TimelineRegistry::new(),
            attachments: Arc::new(Mutex::new(AttachmentAllowlist::default())),
            unrestricted_attachments: false,
        }
    }

    /// Do not start account actors on [`crate::AppEngine::start`].
    pub fn without_background(mut self) -> Self {
        self.background = false;
        self
    }

    /// Accept any attachment path in `SendAttachment`. Headless tests only; the default is strict.
    pub fn allow_unrestricted_attachment_paths(mut self) -> Self {
        self.unrestricted_attachments = true;
        self
    }

    /// Let `SendAttachment` read exactly this file. Fails if the file does not exist.
    pub fn allow_attachment_path(&self, path: &Path) -> io::Result<()> {
        let canonical = path.canonicalize()?;
        self.allowlist().files.insert(canonical);
        Ok(())
    }

    /// Let `SendAttachment` read any file under this directory. Fails if the directory does not exist.
    pub fn allow_attachment_root(&self, root: &Path) -> io::Result<()> {
        let canonical = root.canonicalize()?;
        self.allowlist().roots.push(canonical);
        Ok(())
    }

    /// Whether `SendAttachment` may read `path`: a registered file, or a file under a registered root.
    /// Symlinks are resolved first, so a link cannot escape a registered root.
    pub fn attachment_path_allowed(&self, path: &Path) -> bool {
        if self.unrestricted_attachments {
            return true;
        }
        let Ok(canonical) = path.canonicalize() else {
            return false;
        };
        let list = self.allowlist();
        list.files.contains(&canonical) || list.roots.iter().any(|root| canonical.starts_with(root))
    }

    /// Directory holding the downloaded-media cache.
    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    /// Directory holding one sub-directory per connected account.
    pub fn accounts_dir(&self) -> PathBuf {
        self.data_dir.join("accounts")
    }

    /// Deletes account directories that no stored account owns.
    async fn remove_orphan_account_dirs(&self) {
        let Ok(accounts) = self.store.list_accounts().await else {
            return;
        };
        let known: HashSet<String> = accounts
            .into_iter()
            .map(|record| record.account_id)
            .collect();
        let Ok(entries) = std::fs::read_dir(self.accounts_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if known.contains(name) {
                continue;
            }
            match std::fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "cannot remove an orphaned account directory"
                ),
            }
        }
    }

    fn allowlist(&self) -> MutexGuard<'_, AttachmentAllowlist> {
        self.attachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads the store key, generating and persisting it on first start.
async fn store_key(secrets: &dyn SecretStore) -> Result<[u8; STORE_KEY_LEN], EngineError> {
    match secrets.get_secret(STORE_KEY_SECRET).await {
        Ok(encoded) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|err| {
                    EngineError::internal_error(format!(
                        "the stored database key is not valid base64: {err}"
                    ))
                })?;
            let key: [u8; STORE_KEY_LEN] = bytes.as_slice().try_into().map_err(|_| {
                EngineError::internal_error(format!(
                    "the stored database key is {} bytes, expected {STORE_KEY_LEN}",
                    bytes.len()
                ))
            })?;
            Ok(key)
        }
        Err(StorageError::SecretNotFound(_)) => {
            let mut key = [0u8; STORE_KEY_LEN];
            getrandom::fill(&mut key).map_err(|err| {
                EngineError::internal_error(format!("cannot generate the database key: {err}"))
            })?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(key);
            secrets
                .set_secret(STORE_KEY_SECRET, &encoded)
                .await
                .map_err(|err| {
                    EngineError::internal_error(format!("cannot store the database key: {err}"))
                })?;
            Ok(key)
        }
        Err(err) => Err(EngineError::internal_error(format!(
            "cannot read the database key: {err}"
        ))),
    }
}
