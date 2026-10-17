//! Persistent index of logged-in accounts (`<data_dir>/accounts.json`).
//!
//! The index holds no secrets: access tokens live in the [`SecretStore`] under the
//! account's `store_id` namespace, and the Matrix store is encrypted with a key kept there too.

use alloy_protocol::{Account, AccountId, UserId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountEntry {
    pub user_id: String,
    pub homeserver_url: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// Directory name under `<data_dir>/accounts/` and secret namespace.
    pub store_id: String,
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
    fn corrupt_index_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("accounts.json"), "{not json").unwrap();
        assert!(AccountIndex::new(dir.path()).list().is_empty());
    }
}
