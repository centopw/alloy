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
