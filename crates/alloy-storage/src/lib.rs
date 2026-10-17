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
    fn save(&self, map: &HashMap<String, String>) -> Result<(), StorageError> {
        let text = serde_json::to_string(map)
            .map_err(|e| StorageError::DatabaseError(format!("encode secrets: {e}")))?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
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

/// OS Keychain-backed secret store using `keyring` crate.
#[cfg(feature = "keyring")]
pub struct KeyringSecretStore {
    service: String,
}

#[cfg(feature = "keyring")]
impl KeyringSecretStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn get_entry(&self, key: &str) -> Result<keyring::Entry, StorageError> {
        keyring::Entry::new(&self.service, key)
            .map_err(|e| StorageError::KeyringError(e.to_string()))
    }
}

#[cfg(feature = "keyring")]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let entry = self.get_entry(key)?;
        entry
            .set_password(value)
            .map_err(|e| StorageError::KeyringError(e.to_string()))
    }

    async fn get_secret(&self, key: &str) -> Result<String, StorageError> {
        let entry = self.get_entry(key)?;
        entry
            .get_password()
            .map_err(|e| StorageError::KeyringError(e.to_string()))
    }

    async fn delete_secret(&self, key: &str) -> Result<(), StorageError> {
        let entry = self.get_entry(key)?;
        entry
            .delete_credential()
            .map_err(|e| StorageError::KeyringError(e.to_string()))
    }

    async fn has_secret(&self, key: &str) -> Result<bool, StorageError> {
        let entry = self.get_entry(key)?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(StorageError::KeyringError(e.to_string())),
        }
    }
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
