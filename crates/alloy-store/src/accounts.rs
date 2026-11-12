//! Account rows, their sealed secrets, and per-account sync cursors.

use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::Store;
use crate::crypto;
use crate::error::StoreError;
use crate::model::{AccountRecord, AccountState, NewAccount, now_ms};
use crate::rows;

impl Store {
    pub async fn insert_account(&self, new: NewAccount) -> Result<AccountRecord, StoreError> {
        let now = now_ms();
        let stored = AccountRecord {
            account_id: Uuid::new_v4().to_string(),
            network: new.network,
            remote_user_id: new.remote_user_id,
            display_name: new.display_name,
            avatar_media_id: None,
            state: new.state,
            created_at: now,
            updated_at: now,
        };
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO accounts
                     (account_id, network, remote_user_id, display_name, avatar_media_id, state,
                      created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    stored.account_id,
                    stored.network,
                    stored.remote_user_id,
                    stored.display_name,
                    stored.avatar_media_id,
                    stored.state,
                    stored.created_at,
                    stored.updated_at
                ],
            )?;
            Ok(stored)
        })
        .await
    }

    pub async fn get_account(&self, account_id: &str) -> Result<Option<AccountRecord>, StoreError> {
        let account_id = account_id.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT * FROM accounts WHERE account_id = ?1",
                    [account_id],
                    rows::account,
                )
                .optional()?)
        })
        .await
    }

    pub async fn list_accounts(&self) -> Result<Vec<AccountRecord>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM accounts ORDER BY created_at ASC, account_id ASC")?;
            let rows = stmt.query_map([], rows::account)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    pub async fn update_account_state(
        &self,
        account_id: &str,
        state: AccountState,
    ) -> Result<(), StoreError> {
        let account_id = account_id.to_owned();
        self.with_conn(move |conn| {
            let updated = conn.execute(
                "UPDATE accounts SET state = ?2, updated_at = ?3 WHERE account_id = ?1",
                rusqlite::params![account_id, state, now_ms()],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("account"));
            }
            Ok(())
        })
        .await
    }

    /// Deletes the account and, through `ON DELETE CASCADE`, its secrets, sync state, chats,
    /// messages, reactions, outbox rows and events.
    pub async fn delete_account(&self, account_id: &str) -> Result<(), StoreError> {
        let account_id = account_id.to_owned();
        self.with_conn(move |conn| {
            let deleted =
                conn.execute("DELETE FROM accounts WHERE account_id = ?1", [account_id])?;
            if deleted == 0 {
                return Err(StoreError::NotFound("account"));
            }
            Ok(())
        })
        .await
    }

    /// Encrypt `plaintext` with the device key and store it under `account_id`.
    pub async fn put_secret(&self, account_id: &str, plaintext: &[u8]) -> Result<(), StoreError> {
        let key = self.secret_key;
        let account = account_id.to_owned();
        let plaintext = plaintext.to_vec();
        self.with_conn(move |conn| {
            let sealed = crypto::seal(&key, account.as_bytes(), &plaintext)?;
            conn.execute(
                "INSERT INTO account_secrets (account_id, nonce, ciphertext, key_version, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (account_id) DO UPDATE SET
                     nonce = excluded.nonce,
                     ciphertext = excluded.ciphertext,
                     key_version = excluded.key_version,
                     updated_at = excluded.updated_at",
                rusqlite::params![
                    account,
                    sealed.nonce,
                    sealed.ciphertext,
                    crypto::KEY_VERSION,
                    now_ms()
                ],
            )?;
            Ok(())
        })
        .await
    }

    /// Decrypt the stored secret. A wrong device key is reported as [`StoreError::Decrypt`].
    pub async fn get_secret(&self, account_id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let key = self.secret_key;
        let account = account_id.to_owned();
        self.with_conn(move |conn| {
            let stored = conn
                .query_row(
                    "SELECT nonce, ciphertext FROM account_secrets WHERE account_id = ?1",
                    [&account],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()?;
            let Some((nonce, ciphertext)) = stored else {
                return Ok(None);
            };
            let plaintext = crypto::open(&key, account.as_bytes(), &nonce, &ciphertext, &account)?;
            Ok(Some(plaintext))
        })
        .await
    }

    pub async fn set_sync_cursor(
        &self,
        account_id: &str,
        scope: &str,
        cursor: &str,
    ) -> Result<(), StoreError> {
        let account_id = account_id.to_owned();
        let scope = scope.to_owned();
        let cursor = cursor.to_owned();
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO sync_state (account_id, scope, cursor, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (account_id, scope) DO UPDATE SET
                     cursor = excluded.cursor,
                     updated_at = excluded.updated_at",
                rusqlite::params![account_id, scope, cursor, now_ms()],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_sync_cursor(
        &self,
        account_id: &str,
        scope: &str,
    ) -> Result<Option<String>, StoreError> {
        let account_id = account_id.to_owned();
        let scope = scope.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT cursor FROM sync_state WHERE account_id = ?1 AND scope = ?2",
                    rusqlite::params![account_id, scope],
                    |row| row.get(0),
                )
                .optional()?)
        })
        .await
    }
}
