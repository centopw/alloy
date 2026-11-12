//! Chat projection and user-owned chat state.

use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::Store;
use crate::error::StoreError;
use crate::events;
use crate::model::{ChatFlagsUpdate, ChatRecord, ChatUpsert, now_ms};
use crate::rows;

impl Store {
    /// Insert a chat discovered by sync, or refresh the remote-derived columns of an existing one.
    /// User-owned flags and counters are left untouched. Returns the local `chat_id`.
    pub async fn upsert_chat(&self, chat: ChatUpsert) -> Result<String, StoreError> {
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let chat_id: String = tx.query_row(
                "INSERT INTO chats
                     (chat_id, account_id, remote_chat_id, kind, title, avatar_media_id,
                      last_message_id, last_activity_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (account_id, remote_chat_id) DO UPDATE SET
                     kind = excluded.kind,
                     title = COALESCE(excluded.title, chats.title),
                     avatar_media_id = COALESCE(excluded.avatar_media_id, chats.avatar_media_id),
                     last_message_id = COALESCE(excluded.last_message_id, chats.last_message_id),
                     last_activity_at = CASE
                         WHEN excluded.last_activity_at IS NULL THEN chats.last_activity_at
                         WHEN chats.last_activity_at IS NULL THEN excluded.last_activity_at
                         WHEN excluded.last_activity_at > chats.last_activity_at
                             THEN excluded.last_activity_at
                         ELSE chats.last_activity_at
                     END
                 RETURNING chat_id",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    chat.account_id,
                    chat.remote_chat_id,
                    chat.kind,
                    chat.title,
                    chat.avatar_media_id,
                    chat.last_message_id,
                    chat.last_activity_at
                ],
                |row| row.get(0),
            )?;
            let record = rows::chat_by_id(&tx, &chat_id)?;
            events::append(
                &tx,
                &record.account_id,
                Some(record.chat_id.as_str()),
                "chat.upsert",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record.chat_id)
        })
        .await
    }

    pub async fn get_chat(&self, chat_id: &str) -> Result<Option<ChatRecord>, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT * FROM chats WHERE chat_id = ?1",
                    [chat_id],
                    rows::chat,
                )
                .optional()?)
        })
        .await
    }

    /// Chats for one account (or all accounts), pinned first, then most recent activity.
    pub async fn list_chats(
        &self,
        account_id: Option<String>,
        archived: bool,
        limit: u32,
    ) -> Result<Vec<ChatRecord>, StoreError> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM chats
                 WHERE (?1 IS NULL OR account_id = ?1) AND is_archived = ?2
                 ORDER BY is_pinned DESC, COALESCE(last_activity_at, 0) DESC, chat_id ASC
                 LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![account_id, archived, limit], rows::chat)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Apply user-owned chat state; unspecified fields keep their stored value.
    pub async fn update_chat_flags(
        &self,
        chat_id: &str,
        update: ChatFlagsUpdate,
    ) -> Result<ChatRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let mut record = rows::chat_by_id(&tx, &chat_id)?;
            if let Some(value) = update.is_pinned {
                record.is_pinned = value;
            }
            if let Some(value) = update.is_archived {
                record.is_archived = value;
            }
            if let Some(value) = update.is_marked_unread {
                record.is_marked_unread = value;
            }
            if let Some(value) = update.muted_until {
                record.is_muted = value.is_some();
                record.muted_until = value;
            }
            if let Some(value) = update.draft {
                record.draft = value;
            }
            if let Some(value) = update.tags {
                record.local_tags = value;
            }
            if let Some(value) = update.note {
                record.note = value;
            }

            let tags = serde_json::to_string(&record.local_tags)?;
            tx.execute(
                "UPDATE chats SET
                     is_pinned = ?2,
                     is_archived = ?3,
                     is_muted = ?4,
                     muted_until = ?5,
                     is_marked_unread = ?6,
                     draft = ?7,
                     local_tags = ?8,
                     note = ?9
                 WHERE chat_id = ?1",
                rusqlite::params![
                    chat_id,
                    record.is_pinned,
                    record.is_archived,
                    record.is_muted,
                    record.muted_until,
                    record.is_marked_unread,
                    record.draft,
                    tags,
                    record.note
                ],
            )?;
            events::append(
                &tx,
                &record.account_id,
                Some(record.chat_id.as_str()),
                "chat.flags",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }
}
