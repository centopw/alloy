use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub use alloy_protocol::{AccountId, RoomId};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Secret not found: {0}")]
    SecretNotFound(String),
    #[error("Keyring error: {0}")]
    KeyringError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Trait for secure secret storage (passphrases, keys, tokens).
/// Implementations may use OS keyrings or in-memory storage for testing.
#[async_trait::async_trait]
pub trait SecretStore: Send + Sync + 'static {
    /// Store a secret identified by a key.
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Retrieve a secret by key.
    async fn get_secret(&self, key: &str) -> Result<String, StorageError>;

    /// Delete a secret by key.
    async fn delete_secret(&self, key: &str) -> Result<(), StorageError>;

    /// Check if a secret exists.
    async fn has_secret(&self, key: &str) -> Result<bool, StorageError>;
}

#[async_trait::async_trait]
impl<T: SecretStore + ?Sized> SecretStore for Arc<T> {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError> {
        (**self).set_secret(key, value).await
    }
    async fn get_secret(&self, key: &str) -> Result<String, StorageError> {
        (**self).get_secret(key).await
    }
    async fn delete_secret(&self, key: &str) -> Result<(), StorageError> {
        (**self).delete_secret(key).await
    }
    async fn has_secret(&self, key: &str) -> Result<bool, StorageError> {
        (**self).has_secret(key).await
    }
}

/// In-memory secret store for testing.
#[derive(Clone)]
pub struct InMemorySecretStore {
    secrets: Arc<RwLock<HashMap<String, String>>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self {
            secrets: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SecretStore for InMemorySecretStore {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.secrets
            .write()
            .await
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn get_secret(&self, key: &str) -> Result<String, StorageError> {
        self.secrets
            .read()
            .await
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::SecretNotFound(key.to_string()))
    }

    async fn delete_secret(&self, key: &str) -> Result<(), StorageError> {
        self.secrets.write().await.remove(key);
        Ok(())
    }

    async fn has_secret(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.secrets.read().await.contains_key(key))
    }
}

/// Secret store backed by a JSON file with owner-only permissions.
///
/// Used where no OS keyring exists (Android). The file lives in the app-private data directory,
/// which the OS sandboxes and encrypts at rest (file-based encryption); other apps and
/// unprivileged users cannot read it. Wrapping the values with an Android Keystore key is a
/// later hardening step.
pub struct FileSecretStore {
    path: std::path::PathBuf,
    /// Serialises read-modify-write cycles so concurrent writers cannot lose updates.
    lock: tokio::sync::Mutex<()>,
}

impl FileSecretStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    fn load(&self) -> Result<HashMap<String, String>, StorageError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| StorageError::DatabaseError(format!("corrupt secret file: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write through a temp file and rename so a crash never leaves a truncated file.
    ///
    /// The temp file is created exclusively with owner-only permissions, so the secrets are never
    /// readable by other users, not even briefly between create and chmod.
    fn save(&self, map: &HashMap<String, String>) -> Result<(), StorageError> {
        let text = serde_json::to_string(map)
            .map_err(|e| StorageError::DatabaseError(format!("encode secrets: {e}")))?;
        let tmp = self.path.with_extension("tmp");
        // A temp file left by a crashed write must not block this one or keep its old contents.
        match std::fs::remove_file(&tmp) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let result =
            write_private(&tmp, text.as_bytes()).and_then(|()| std::fs::rename(&tmp, &self.path));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(result?)
    }

    /// Adds the secrets this file does not hold yet, in one atomic write, and never overwrites one
    /// it already has.
    ///
    /// Used to move keys out of the OS keychain once: the file is authoritative afterwards, and a
    /// second import is a no-op.
    pub fn import(&self, entries: &[(&str, String)]) -> Result<(), StorageError> {
        let mut map = self.load()?;
        let mut changed = false;
        for (key, value) in entries {
            if !map.contains_key(*key) {
                map.insert((*key).to_string(), value.clone());
                changed = true;
            }
        }
        if changed {
            self.save(&map)?;
        }
        Ok(())
    }

    /// Whether every `key` is already in the file, without touching any other backend.
    pub fn contains_all(&self, keys: &[&str]) -> Result<bool, StorageError> {
        let map = self.load()?;
        Ok(keys.iter().all(|key| map.contains_key(*key)))
    }
}

/// Creates `path` exclusively with owner-only permissions (0600 on unix), writes `bytes`, and fsyncs.
pub fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[async_trait::async_trait]
impl SecretStore for FileSecretStore {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let _guard = self.lock.lock().await;
        let mut map = self.load()?;
        map.insert(key.to_string(), value.to_string());
        self.save(&map)
    }

    async fn get_secret(&self, key: &str) -> Result<String, StorageError> {
        let _guard = self.lock.lock().await;
        self.load()?
            .remove(key)
            .ok_or_else(|| StorageError::SecretNotFound(key.to_string()))
    }

    async fn delete_secret(&self, key: &str) -> Result<(), StorageError> {
        let _guard = self.lock.lock().await;
        let mut map = self.load()?;
        if map.remove(key).is_some() {
            self.save(&map)?;
        }
        Ok(())
    }

    async fn has_secret(&self, key: &str) -> Result<bool, StorageError> {
        let _guard = self.lock.lock().await;
        Ok(self.load()?.contains_key(key))
    }
}

/// OS keychain-backed secret store using the `keyring` crate.
///
/// Only compiled on desktop targets whose keyring backend is enabled in `Cargo.toml`. Constructing
/// it verifies that the platform keychain answers, so a missing backend fails closed instead of
/// silently falling back to keyring's in-memory mock.
///
/// Every secret is read from the platform keychain at most once per process: on macOS the keychain
/// asks the user for the login password whenever the reading binary is not the one that created the
/// item (each unsigned development build and each release has a new signature), so a second read of
/// the same item would prompt again for no benefit. Writes and deletions through this instance
/// update the cache; a second instance reads the keychain itself, so use one store per process.
#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
pub struct KeyringSecretStore {
    service: String,
    /// Resolved secrets: `Some` is a value the keychain holds, `None` an entry it does not.
    cache: tokio::sync::RwLock<HashMap<String, Option<String>>>,
}

#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
const KEYCHAIN_PROBE_KEY: &str = "__alloy_keychain_probe__";

#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
impl KeyringSecretStore {
    /// Opens the keychain for `service`, failing if the platform backend cannot answer.
    ///
    /// The probe is a single read of an entry this app never writes: a missing entry proves the
    /// backend is live, and it cannot prompt (macOS asks about existing items, not absent ones).
    /// Nothing is written, so opening the store never leaves a stray keychain item behind.
    pub fn new(service: &str) -> Result<Self, StorageError> {
        let store = Self {
            service: service.to_string(),
            cache: tokio::sync::RwLock::new(HashMap::new()),
        };
        store.probe()?;
        Ok(store)
    }

    fn probe(&self) -> Result<(), StorageError> {
        match self.get_entry(KEYCHAIN_PROBE_KEY)?.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn get_entry(&self, key: &str) -> Result<keyring::Entry, StorageError> {
        keyring::Entry::new(&self.service, key).map_err(keyring_error)
    }

    /// Reads one secret through the cache, touching the platform keychain at most once per key.
    async fn resolve(&self, key: &str) -> Result<Option<String>, StorageError> {
        if let Some(hit) = self.cache.read().await.get(key) {
            return Ok(hit.clone());
        }
        let value = match self.get_entry(key)?.get_password() {
            Ok(value) => Some(value),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => return Err(keyring_error(e)),
        };
        self.cache
            .write()
            .await
            .insert(key.to_string(), value.clone());
        Ok(value)
    }
}

#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
fn keyring_error(e: keyring::Error) -> StorageError {
    StorageError::KeyringError(e.to_string())
}

#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.get_entry(key)?
            .set_password(value)
            .map_err(keyring_error)?;
        self.cache
            .write()
            .await
            .insert(key.to_string(), Some(value.to_string()));
        Ok(())
    }

    async fn get_secret(&self, key: &str) -> Result<String, StorageError> {
        self.resolve(key)
            .await?
            .ok_or_else(|| StorageError::SecretNotFound(key.to_string()))
    }

    async fn delete_secret(&self, key: &str) -> Result<(), StorageError> {
        let result = match self.get_entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        };
        if result.is_ok() {
            self.cache.write().await.insert(key.to_string(), None);
        }
        result
    }

    async fn has_secret(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.resolve(key).await?.is_some())
    }
}

/// Moves secrets that only the OS keychain holds into an owner-only secret file, once.
///
/// The file is authoritative afterwards: keys it already holds are skipped without touching the
/// keychain, so a second start reads nothing from the platform store and cannot prompt. An error
/// means the keychain refused a read (locked, access denied, no backend) and the caller must not
/// mint a replacement secret, because a database sealed with the missing key would become
/// unreadable.
#[cfg(all(
    feature = "keyring",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
pub fn import_missing_from_keyring(
    service: &str,
    keys: &[&str],
    file: &FileSecretStore,
) -> Result<(), StorageError> {
    if file.contains_all(keys)? {
        return Ok(());
    }
    let mut imported: Vec<(&str, String)> = Vec::new();
    for key in keys {
        if file.contains_all(&[*key])? {
            continue;
        }
        let entry = keyring::Entry::new(service, key).map_err(keyring_error)?;
        match entry.get_password() {
            Ok(value) => imported.push((*key, value)),
            // The keychain never had it: nothing to migrate, and no prompt was shown.
            Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(keyring_error(e)),
        }
    }
    file.import(&imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_store_persists_across_instances_and_restricts_access() {
        let dir = std::env::temp_dir().join(format!("alloy-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.json");
        {
            let store = FileSecretStore::new(&path);
            assert!(!store.has_secret("a").await.unwrap());
            store.set_secret("a", "1").await.unwrap();
            store.set_secret("b", "2").await.unwrap();
            store.delete_secret("b").await.unwrap();
        }
        let reopened = FileSecretStore::new(&path);
        assert_eq!(reopened.get_secret("a").await.unwrap(), "1");
        assert!(!reopened.has_secret("b").await.unwrap());
        assert!(matches!(
            reopened.get_secret("b").await,
            Err(StorageError::SecretNotFound(_))
        ));
        assert!(
            !dir.join("secrets.tmp").exists(),
            "temp file must not outlive a save"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o077,
                0,
                "secret file must not be group/world accessible"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The file is authoritative after a migration: importing again never overwrites a key it
    /// already holds, and `contains_all` answers without a write.
    #[tokio::test]
    async fn file_store_import_keeps_the_existing_values() {
        let dir = std::env::temp_dir().join(format!("alloy-secrets-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.json");
        let store = FileSecretStore::new(&path);
        assert!(!store.contains_all(&["a"]).unwrap());
        store.import(&[("a", "1".to_string())]).unwrap();
        assert!(store.contains_all(&["a"]).unwrap());
        assert!(!store.contains_all(&["a", "b"]).unwrap());
        store
            .import(&[("a", "2".to_string()), ("b", "3".to_string())])
            .unwrap();
        assert_eq!(store.get_secret("a").await.unwrap(), "1");
        assert_eq!(store.get_secret("b").await.unwrap(), "3");
        assert!(store.contains_all(&["a", "b"]).unwrap());
        assert!(
            !dir.join("secrets.tmp").exists(),
            "temp file must not outlive an import"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The store answers from the platform keychain and keeps the answer: one platform read per key
    /// per process, which is what stops macOS from asking for the login password more than once.
    /// A separate instance still reads the keychain, so a value written before it opened is visible.
    #[cfg(all(
        feature = "keyring",
        any(target_os = "linux", target_os = "macos", target_os = "windows")
    ))]
    #[tokio::test]
    async fn keyring_round_trips_and_reads_each_key_once() {
        let service = format!("chat.alloy.test.{}", std::process::id());
        let store = match KeyringSecretStore::new(&service) {
            Ok(store) => store,
            Err(e) => {
                eprintln!("skipping keyring round-trip test, no usable keychain: {e}");
                return;
            }
        };
        assert!(!store.has_secret("round-trip").await.unwrap());
        store.set_secret("round-trip", "v1").await.unwrap();
        assert_eq!(store.get_secret("round-trip").await.unwrap(), "v1");

        let reopened = KeyringSecretStore::new(&service).unwrap();
        assert_eq!(reopened.get_secret("round-trip").await.unwrap(), "v1");

        // A deletion through the instance that cached the value is visible to it immediately.
        store.delete_secret("round-trip").await.unwrap();
        assert!(matches!(
            store.get_secret("round-trip").await,
            Err(StorageError::SecretNotFound(_))
        ));
        reopened.delete_secret("round-trip").await.unwrap();
    }

    #[tokio::test]
    async fn test_in_memory_secret_store() {
        let store = InMemorySecretStore::new();

        // Set and get
        store.set_secret("test_key", "test_value").await.unwrap();
        let value = store.get_secret("test_key").await.unwrap();
        assert_eq!(value, "test_value");

        // Has
        assert!(store.has_secret("test_key").await.unwrap());
        assert!(!store.has_secret("nonexistent").await.unwrap());

        // Delete
        store.delete_secret("test_key").await.unwrap();
        assert!(!store.has_secret("test_key").await.unwrap());

        // Get nonexistent
        let result = store.get_secret("nonexistent").await;
        assert!(result.is_err());
    }
}
