//! Persistent index of logged-in accounts (`<data_dir>/accounts.json`) and session restoration.
//!
//! The index holds no secrets: access tokens live in the [`SecretStore`] under the
//! account's `store_id` namespace, and the Matrix store is encrypted with a key kept there too.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alloy_protocol::{Account, AccountId, UserId};
use alloy_storage::{FileSecretStore, SecretStore, StorageError};
use serde::{Deserialize, Serialize};

use crate::client::AlloyClient;
use crate::{AlloyError, AlloyResult};

/// Entry in `<data_dir>/accounts.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountEntry {
    pub user_id: String,
    pub homeserver_url: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// Directory name under `<data_dir>/accounts/` and secret namespace.
    pub store_id: String,
    /// Whether this account is marked as active.
    #[serde(default)]
    pub is_active: bool,
}

impl AccountEntry {
    pub fn to_account(&self) -> Account {
        Account {
            id: AccountId(self.user_id.clone()),
            user_id: UserId(self.user_id.clone()),
            display_name: self.display_name.clone(),
            avatar_url: self.avatar_url.clone(),
            homeserver_url: self.homeserver_url.clone(),
        }
    }
}

/// Persistent index of logged-in accounts.
#[derive(Debug, Clone)]
pub struct AccountIndex {
    root: PathBuf,
}

impl AccountIndex {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.to_path_buf(),
        }
    }

    fn file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn store_dir(&self, store_id: &str) -> PathBuf {
        self.root.join("accounts").join(store_id)
    }

    pub fn list(&self) -> Vec<AccountEntry> {
        std::fs::read_to_string(self.file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, account_id: &str) -> Option<AccountEntry> {
        self.list().into_iter().find(|e| e.user_id == account_id)
    }

    /// Return the active account, or the first one listed if none is explicitly marked active.
    pub fn active(&self) -> Option<AccountEntry> {
        let list = self.list();
        list.iter()
            .find(|e| e.is_active)
            .cloned()
            .or_else(|| list.into_iter().next())
    }

    /// Mark an account as active (and all others inactive).
    pub fn set_active(&self, account_id: &str) -> std::io::Result<()> {
        let mut all = self.list();
        for entry in &mut all {
            entry.is_active = entry.user_id == account_id;
        }
        self.write(&all)
    }

    /// Insert or replace an entry keyed by `user_id`; returns the replaced entry, if any.
    pub fn upsert(&self, entry: AccountEntry) -> std::io::Result<Option<AccountEntry>> {
        let mut all = self.list();
        let old = all
            .iter()
            .position(|e| e.user_id == entry.user_id)
            .map(|i| all.remove(i));
        all.push(entry);
        self.write(&all)?;
        Ok(old)
    }

    pub fn remove(&self, account_id: &str) -> std::io::Result<Option<AccountEntry>> {
        let mut all = self.list();
        let old = all
            .iter()
            .position(|e| e.user_id == account_id)
            .map(|i| all.remove(i));
        self.write(&all)?;
        Ok(old)
    }

    fn write(&self, all: &[AccountEntry]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let tmp = self.root.join("accounts.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(all)?)?;
        std::fs::rename(tmp, self.file())
    }
}

/// Open the default secret store for `data_dir`.
///
/// On Android, or whenever `<data_dir>/secrets.json` exists, uses `FileSecretStore`.
/// On desktop OSes (macOS, Windows, Linux) without an existing `secrets.json`,
/// tries the OS keyring, falling back to `FileSecretStore` on Linux if keyring is unavailable.
pub fn open_default_secret_store(data_dir: &Path) -> Result<Arc<dyn SecretStore>, StorageError> {
    if data_dir.join("secrets.json").exists() {
        return Ok(Arc::new(FileSecretStore::new(
            data_dir.join("secrets.json"),
        )));
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        match alloy_storage::KeyringSecretStore::new("chat.alloy") {
            Ok(store) => return Ok(Arc::new(store)),
            Err(e) if cfg!(target_os = "linux") => {
                tracing::warn!(
                    error = %e,
                    "OS keyring unavailable; storing secrets in an owner-only file"
                );
            }
            Err(e) => return Err(e),
        }
    }

    Ok(Arc::new(FileSecretStore::new(
        data_dir.join("secrets.json"),
    )))
}

/// Restore a session for an account in `data_dir`.
///
/// If `account_id` is specified, restores that account.
/// Otherwise restores the active account (or the first account in `accounts.json`).
/// If `secret_store` is omitted, uses [`open_default_secret_store`].
pub async fn restore_account(
    data_dir: &Path,
    account_id: Option<&str>,
    secret_store: Option<Box<dyn SecretStore>>,
) -> AlloyResult<(AlloyClient, AccountEntry)> {
    let index = AccountIndex::new(data_dir);
    let entry = match account_id {
        Some(id) => index
            .get(id)
            .ok_or_else(|| AlloyError::AccountNotFound(format!("account {id} not found")))?,
        None => index
            .active()
            .ok_or_else(|| AlloyError::Auth("no account found in data dir".to_string()))?,
    };

    let store = match secret_store {
        Some(s) => s,
        None => Box::new(open_default_secret_store(data_dir)?) as Box<dyn SecretStore>,
    };

    let store_dir = index.store_dir(&entry.store_id);
    let client = AlloyClient::open(
        &entry.homeserver_url,
        &store_dir,
        Some(store),
        Some(&entry.store_id),
    )
    .await?;

    if !client.restore_session().await? {
        return Err(AlloyError::Auth(
            "no stored session; log in again".to_string(),
        ));
    }

    Ok((client, entry))
}

/// Convenience function to restore the active account from `data_dir`.
pub async fn restore_active_account(
    data_dir: &Path,
    secret_store: Option<Box<dyn SecretStore>>,
) -> AlloyResult<(AlloyClient, AccountEntry)> {
    restore_account(data_dir, None, secret_store).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(user: &str, store: &str) -> AccountEntry {
        AccountEntry {
            user_id: user.into(),
            homeserver_url: "https://hs.example/".into(),
            display_name: None,
            avatar_url: None,
            store_id: store.into(),
            is_active: false,
        }
    }

    #[test]
    fn upsert_replaces_same_user_and_remove_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let idx = AccountIndex::new(dir.path());
        assert!(idx.list().is_empty());
        assert!(idx.upsert(entry("@a:hs", "s1")).unwrap().is_none());
        idx.upsert(entry("@b:hs", "s2")).unwrap();
        let old = idx.upsert(entry("@a:hs", "s3")).unwrap().unwrap();
        assert_eq!(old.store_id, "s1");
        assert_eq!(idx.list().len(), 2);
        assert_eq!(idx.get("@a:hs").unwrap().store_id, "s3");
        assert_eq!(idx.remove("@b:hs").unwrap().unwrap().store_id, "s2");
        assert!(idx.get("@b:hs").is_none());
    }

    #[test]
    fn active_account_selection() {
        let dir = tempfile::tempdir().unwrap();
        let idx = AccountIndex::new(dir.path());
        assert!(idx.active().is_none());

        let mut e1 = entry("@a:hs", "s1");
        e1.is_active = false;
        let mut e2 = entry("@b:hs", "s2");
        e2.is_active = true;

        idx.upsert(e1).unwrap();
        // Fallback to first if none marked active yet
        assert_eq!(idx.active().unwrap().user_id, "@a:hs");

        idx.upsert(e2).unwrap();
        assert_eq!(idx.active().unwrap().user_id, "@b:hs");

        idx.set_active("@a:hs").unwrap();
        assert_eq!(idx.active().unwrap().user_id, "@a:hs");
    }

    #[test]
    fn corrupt_index_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("accounts.json"), "{not json").unwrap();
        assert!(AccountIndex::new(dir.path()).list().is_empty());
    }
}
