use crate::registry::SubscriptionRegistry;
use crate::sink::EventSink;
use alloy_core::AlloyClient;
use alloy_storage::SecretStore;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::RwLock;

/// Files and directories `SendAttachment` may read. Paths are canonical.
#[derive(Default)]
struct AttachmentAllowlist {
    files: HashSet<PathBuf>,
    roots: Vec<PathBuf>,
}

/// Shared engine context passed to command handlers.
#[derive(Clone)]
pub struct EngineContext {
    pub client: Arc<RwLock<Option<Arc<AlloyClient>>>>,
    pub sink: EventSink,
    pub data_dir: PathBuf,
    pub secrets: Arc<dyn SecretStore>,
    pub subscriptions: SubscriptionRegistry,
    /// Start sync, recovery and verification watchers after login (disable for headless use).
    pub background: bool,
    attachments: Arc<Mutex<AttachmentAllowlist>>,
    unrestricted_attachments: bool,
}

impl EngineContext {
    pub fn new(data_dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            sink: EventSink::new(),
            data_dir,
            secrets,
            subscriptions: SubscriptionRegistry::new(),
            background: true,
            attachments: Arc::new(Mutex::new(AttachmentAllowlist::default())),
            unrestricted_attachments: false,
        }
    }

    /// Do not start background sync/watchers after login.
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

    fn allowlist(&self) -> MutexGuard<'_, AttachmentAllowlist> {
        self.attachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl EngineContext {
    /// The active client, or an `auth_failed` error when nobody is logged in.
    pub async fn active_client(&self) -> Result<Arc<AlloyClient>, alloy_protocol::EngineError> {
        self.client
            .read()
            .await
            .clone()
            .ok_or_else(|| alloy_protocol::EngineError::auth_failed("not logged in"))
    }

    /// Replace the active client, cancelling all subscriptions of the previous one.
    pub async fn set_client(&self, client: Option<AlloyClient>) {
        self.subscriptions.clear().await;
        let old = std::mem::replace(&mut *self.client.write().await, client.map(Arc::new));
        if let Some(old) = old {
            old.stop_sync().await;
        }
    }
}
