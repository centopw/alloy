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
                      last_message_id, last_activity_at, parent_remote_chat_id, is_forum)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (account_id, remote_chat_id) DO UPDATE SET
                     kind = excluded.kind,
                     parent_remote_chat_id = COALESCE(excluded.parent_remote_chat_id, chats.parent_remote_chat_id),
                     is_forum = excluded.is_forum OR chats.is_forum,
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
                    chat.last_activity_at,
                    chat.parent_remote_chat_id,
                    chat.is_forum
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

    /// Chats for one account (or all accounts): pinned first in their pin order, then every
    /// other chat by most recent activity.
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
                 ORDER BY is_pinned DESC, pin_order IS NULL, pin_order ASC,
                          COALESCE(last_activity_at, 0) DESC, chat_id ASC
                 LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![account_id, archived, limit], rows::chat)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Every chat that carries at least one tag, across accounts and archived state, oldest tag
    /// stamp first. The configuration bundle export reads them; nothing else needs the whole set.
    pub async fn list_tagged_chats(&self) -> Result<Vec<ChatRecord>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM chats
                 WHERE local_tags IS NOT NULL AND local_tags != '[]'
                 ORDER BY tags_updated_at ASC, chat_id ASC",
            )?;
            let rows = stmt.query_map([], rows::chat)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Every chat across accounts, newest activity first, for the opt-in message mirror.
    ///
    /// The mirror publishes a newest-first window; this order is what decides which chats fit
    /// the payload budget. Archived chats are included because the mirror carries their state
    /// too.
    pub async fn list_mirror_chats(&self) -> Result<Vec<ChatRecord>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM chats
                 ORDER BY COALESCE(last_activity_at, 0) DESC, chat_id ASC",
            )?;
            let rows = stmt.query_map([], rows::chat)?;
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
            if update.is_pinned == Some(true) && record.parent_remote_chat_id.is_some() {
                return Err(StoreError::InvalidValue {
                    field: "is_pinned",
                    value: record.chat_id.clone(),
                });
            }
            if let Some(value) = update.is_pinned
                && value != record.is_pinned
            {
                record.is_pinned = value;
                // A chat pinned now goes to the top, which is what every network does; the
                // others move down one. Unpinning closes the gap it leaves.
                record.pin_order = if value {
                    tx.execute(
                        "UPDATE chats SET pin_order = pin_order + 1
                         WHERE account_id = ?1 AND is_pinned = 1 AND is_archived = 0 AND parent_remote_chat_id IS NULL AND pin_order IS NOT NULL",
                        [&record.account_id],
                    )?;
                    Some(0)
                } else {
                    if let Some(gone) = record.pin_order {
                        tx.execute(
                            "UPDATE chats SET pin_order = pin_order - 1
                             WHERE account_id = ?1 AND is_pinned = 1 AND is_archived = 0 AND parent_remote_chat_id IS NULL AND pin_order > ?2",
                            rusqlite::params![&record.account_id, gone],
                        )?;
                    }
                    None
                };
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
                record.tags_updated_at = update.tags_updated_at.unwrap_or_else(now_ms);
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
                     note = ?9,
                     pin_order = ?10,
                     tags_updated_at = ?11
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
                    record.note,
                    record.pin_order,
                    record.tags_updated_at
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

    /// Replaces the pinned list of one account with `chat_ids`, top to bottom. Every chat named
    /// becomes pinned in that position; a pinned chat of the account that is not named is
    /// unpinned. Ids of other accounts or unknown chats are refused, so a stale list from the
    /// UI never half-applies.
    pub async fn set_pinned_chats(
        &self,
        account_id: &str,
        chat_ids: Vec<String>,
    ) -> Result<Vec<ChatRecord>, StoreError> {
        let account_id = account_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            for chat_id in &chat_ids {
                let record = rows::chat_by_id(&tx, chat_id)?;
                if record.account_id != account_id || record.is_archived || record.parent_remote_chat_id.is_some() {
                    return Err(StoreError::NotFound("chat"));
                }
            }
            tx.execute(
                "UPDATE chats SET is_pinned = 0, pin_order = NULL
                 WHERE account_id = ?1 AND is_pinned = 1 AND is_archived = 0 AND parent_remote_chat_id IS NULL",
                [&account_id],
            )?;
            for (position, chat_id) in chat_ids.iter().enumerate() {
                tx.execute(
                    "UPDATE chats SET is_pinned = 1, pin_order = ?2 WHERE chat_id = ?1",
                    rusqlite::params![chat_id, position as i64],
                )?;
            }
            let mut stmt = tx.prepare(
                "SELECT * FROM chats WHERE account_id = ?1 AND is_pinned = 1 AND is_archived = 0 AND parent_remote_chat_id IS NULL
                 ORDER BY pin_order ASC",
            )?;
            let records = stmt
                .query_map([&account_id], rows::chat)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            let now = now_ms();
            for record in &records {
                events::append(
                    &tx,
                    &record.account_id,
                    Some(record.chat_id.as_str()),
                    "chat.flags",
                    serde_json::to_value(record)?,
                    now,
                )?;
            }
            tx.commit()?;
            Ok(records)
        })
        .await
    }

    /// Sets one pinned chat's position without touching the others, for a position the network
    /// reported with the chat itself. The chat must already be pinned.
    pub async fn set_pin_order(&self, chat_id: &str, pin_order: i64) -> Result<(), StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let updated = conn.execute(
                "UPDATE chats SET pin_order = ?2 WHERE chat_id = ?1 AND is_pinned = 1",
                rusqlite::params![&chat_id, pin_order],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("chat"));
            }
            Ok(())
        })
        .await
    }

    /// Pins a chat at a position the network reported, or unpins it with `None`, without shifting
    /// the other pinned chats: the network's own orders are authoritative, so writing them one
    /// chat at a time must leave each at exactly the position it was given.
    pub async fn set_chat_pin_state(
        &self,
        chat_id: &str,
        pin_order: Option<i64>,
    ) -> Result<ChatRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let mut record = rows::chat_by_id(&tx, &chat_id)?;
            record.is_pinned = pin_order.is_some();
            record.pin_order = pin_order;
            tx.execute(
                "UPDATE chats SET is_pinned = ?2, pin_order = ?3 WHERE chat_id = ?1",
                rusqlite::params![chat_id, record.is_pinned, record.pin_order],
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

    /// The pinned chats of one account, top to bottom.
    pub async fn pinned_chats(&self, account_id: &str) -> Result<Vec<ChatRecord>, StoreError> {
        let account_id = account_id.to_owned();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM chats WHERE account_id = ?1 AND is_pinned = 1 AND is_archived = 0 AND parent_remote_chat_id IS NULL
                 ORDER BY pin_order IS NULL, pin_order ASC, COALESCE(last_activity_at, 0) DESC",
            )?;
            let rows = stmt.query_map([&account_id], rows::chat)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// The topics of one forum, newest activity first. Each topic is a chat of its own; the forum
    /// is only the grouping it is listed under.
    pub async fn list_topics(
        &self,
        account_id: &str,
        forum_remote_chat_id: &str,
    ) -> Result<Vec<ChatRecord>, StoreError> {
        let account_id = account_id.to_owned();
        let forum_remote_chat_id = forum_remote_chat_id.to_owned();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM chats
                 WHERE account_id = ?1 AND parent_remote_chat_id = ?2
                 ORDER BY COALESCE(last_activity_at, 0) DESC, chat_id ASC",
            )?;
            let rows = stmt.query_map(
                rusqlite::params![account_id, forum_remote_chat_id],
                rows::chat,
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Resolve the local chat row for a network chat id within one account.
    pub async fn find_chat(
        &self,
        account_id: &str,
        remote_chat_id: &str,
    ) -> Result<Option<ChatRecord>, StoreError> {
        let account_id = account_id.to_owned();
        let remote_chat_id = remote_chat_id.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT * FROM chats WHERE account_id = ?1 AND remote_chat_id = ?2",
                    rusqlite::params![account_id, remote_chat_id],
                    rows::chat,
                )
                .optional()?)
        })
        .await
    }

    /// Refresh the network-owned chat metadata the engine receives from a connector event.
    /// `None` keeps the stored value; `last_activity_at` only ever moves forward, so an older
    /// chat-list page cannot rewind a chat.
    pub async fn update_chat_meta(
        &self,
        chat_id: &str,
        title: Option<String>,
        avatar_media_id: Option<String>,
        last_message_id: Option<String>,
        last_activity_at: Option<i64>,
    ) -> Result<ChatRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let updated = tx.execute(
                "UPDATE chats SET
                     title = COALESCE(?2, title),
                     avatar_media_id = COALESCE(?3, avatar_media_id),
                     last_message_id = COALESCE(?4, last_message_id),
                     last_activity_at = CASE
                         WHEN ?5 IS NULL THEN last_activity_at
                         WHEN last_activity_at IS NULL OR ?5 > last_activity_at THEN ?5
                         ELSE last_activity_at
                     END
                 WHERE chat_id = ?1",
                rusqlite::params![
                    &chat_id,
                    title,
                    avatar_media_id,
                    last_message_id,
                    last_activity_at
                ],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("chat"));
            }
            let record = rows::chat_by_id(&tx, &chat_id)?;
            events::append(
                &tx,
                &record.account_id,
                Some(record.chat_id.as_str()),
                "chat.updated",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    /// Replace the unread counters with the network's numbers (the network is authoritative for
    /// them; a mute suppresses the badge in the UI, not the count in the store).
    pub async fn set_unread(
        &self,
        chat_id: &str,
        unread_count: i64,
        mention_count: i64,
    ) -> Result<ChatRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let updated = tx.execute(
                "UPDATE chats SET unread_count = ?2, mention_count = ?3 WHERE chat_id = ?1",
                rusqlite::params![&chat_id, unread_count, mention_count],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("chat"));
            }
            let record = rows::chat_by_id(&tx, &chat_id)?;
            events::append(
                &tx,
                &record.account_id,
                Some(record.chat_id.as_str()),
                "chat.updated",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    /// Add one unread message to a chat that the engine did not otherwise re-sync, and one
    /// mention when `mention` is set.
    pub async fn increment_unread(
        &self,
        chat_id: &str,
        mention: bool,
    ) -> Result<ChatRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let updated = tx.execute(
                "UPDATE chats SET
                     unread_count = unread_count + 1,
                     mention_count = mention_count + ?2
                 WHERE chat_id = ?1",
                rusqlite::params![&chat_id, mention],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("chat"));
            }
            let record = rows::chat_by_id(&tx, &chat_id)?;
            events::append(
                &tx,
                &record.account_id,
                Some(record.chat_id.as_str()),
                "chat.updated",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }
}
