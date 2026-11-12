//! Message projection, keyset pagination, reactions and full-text search.

use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::Store;
use crate::error::StoreError;
use crate::events;
use crate::model::{MessageRecord, NewMessage, now_ms};
use crate::rows;

impl Store {
    /// Insert a message, or update the row it resolves to.
    ///
    /// Identity order: an explicit `message_id`, then `txn_id` (a remote echo carries the same
    /// `txn_id` as the queued local row and therefore updates it in place), then
    /// `(chat_id, remote_msg_id)`.
    pub async fn upsert_message(&self, message: NewMessage) -> Result<MessageRecord, StoreError> {
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let message_id = match find_existing(&tx, &message)? {
                Some(message_id) => {
                    update_message(&tx, &message_id, &message)?;
                    message_id
                }
                None => {
                    let message_id = message
                        .message_id
                        .clone()
                        .unwrap_or_else(|| Uuid::new_v4().to_string());
                    insert_message(&tx, &message_id, &message)?;
                    message_id
                }
            };

            let record = rows::message_by_id(&tx, &message_id)?;
            let account_id = rows::account_for_chat(&tx, &record.chat_id)?;
            events::append(
                &tx,
                &account_id,
                Some(record.chat_id.as_str()),
                "message.upsert",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    /// Newest-first keyset pagination. `before` is the `(sent_at, message_id)` of the oldest row
    /// already shown; ties on `sent_at` are broken by `message_id` so pages never overlap or skip.
    pub async fn page_messages(
        &self,
        chat_id: &str,
        before: Option<(i64, String)>,
        limit: u32,
    ) -> Result<Vec<MessageRecord>, StoreError> {
        let chat_id = chat_id.to_owned();
        let before_sent_at = before.as_ref().map(|(sent_at, _)| *sent_at);
        let before_id = before.as_ref().map(|(_, message_id)| message_id.clone());
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM messages
                 WHERE chat_id = ?1
                   AND (?2 IS NULL
                        OR sent_at < ?2
                        OR (sent_at = ?2 AND message_id < ?3))
                 ORDER BY sent_at DESC, message_id DESC
                 LIMIT ?4",
            )?;
            let rows = stmt.query_map(
                rusqlite::params![chat_id, before_sent_at, before_id, limit],
                rows::message,
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Tombstone a message: mark it deleted and drop its searchable content.
    pub async fn mark_deleted(&self, message_id: &str, deleted_at: i64) -> Result<(), StoreError> {
        let message_id = message_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let chat_id: Option<String> = tx
                .query_row(
                    "SELECT chat_id FROM messages WHERE message_id = ?1",
                    [&message_id],
                    |row| row.get(0),
                )
                .optional()?;
            let chat_id = chat_id.ok_or(StoreError::NotFound("message"))?;

            tx.execute(
                "UPDATE messages
                 SET deleted_at = ?2, body = NULL, formatted = NULL, media_id = NULL
                 WHERE message_id = ?1",
                rusqlite::params![message_id, deleted_at],
            )?;

            let account_id = rows::account_for_chat(&tx, &chat_id)?;
            events::append(
                &tx,
                &account_id,
                Some(chat_id.as_str()),
                "message.deleted",
                serde_json::json!({
                    "message_id": message_id,
                    "chat_id": chat_id,
                    "deleted_at": deleted_at,
                }),
                now_ms(),
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn set_reaction(
        &self,
        message_id: &str,
        sender_remote_id: &str,
        emoji: &str,
        reacted_at: i64,
    ) -> Result<(), StoreError> {
        let message_id = message_id.to_owned();
        let sender_remote_id = sender_remote_id.to_owned();
        let emoji = emoji.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let chat_id = message_chat(&tx, &message_id)?;
            tx.execute(
                "INSERT INTO reactions (message_id, sender_remote_id, emoji, reacted_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (message_id, sender_remote_id, emoji) DO UPDATE SET
                     reacted_at = excluded.reacted_at",
                rusqlite::params![message_id, sender_remote_id, emoji, reacted_at],
            )?;

            let account_id = rows::account_for_chat(&tx, &chat_id)?;
            events::append(
                &tx,
                &account_id,
                Some(chat_id.as_str()),
                "reaction.set",
                serde_json::json!({
                    "message_id": message_id,
                    "sender_remote_id": sender_remote_id,
                    "emoji": emoji,
                    "reacted_at": reacted_at,
                }),
                now_ms(),
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Removing a reaction that is not stored is a no-op; no event is emitted.
    pub async fn remove_reaction(
        &self,
        message_id: &str,
        sender_remote_id: &str,
        emoji: &str,
    ) -> Result<(), StoreError> {
        let message_id = message_id.to_owned();
        let sender_remote_id = sender_remote_id.to_owned();
        let emoji = emoji.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let chat_id = message_chat(&tx, &message_id)?;
            let removed = tx.execute(
                "DELETE FROM reactions
                 WHERE message_id = ?1 AND sender_remote_id = ?2 AND emoji = ?3",
                rusqlite::params![message_id, sender_remote_id, emoji],
            )?;
            if removed > 0 {
                let account_id = rows::account_for_chat(&tx, &chat_id)?;
                events::append(
                    &tx,
                    &account_id,
                    Some(chat_id.as_str()),
                    "reaction.removed",
                    serde_json::json!({
                        "message_id": message_id,
                        "sender_remote_id": sender_remote_id,
                        "emoji": emoji,
                    }),
                    now_ms(),
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// FTS5 `MATCH` over message bodies, optionally limited to one account.
    pub async fn search_messages(
        &self,
        query: &str,
        account_id: Option<String>,
        limit: u32,
    ) -> Result<Vec<MessageRecord>, StoreError> {
        let query = query.to_owned();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT m.*
                 FROM messages_fts
                 JOIN messages m ON m.seq = messages_fts.rowid
                 JOIN chats c ON c.chat_id = m.chat_id
                 WHERE messages_fts MATCH ?1 AND (?2 IS NULL OR c.account_id = ?2)
                 ORDER BY m.sent_at DESC, m.message_id DESC
                 LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![query, account_id, limit], rows::message)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }
}

fn find_existing(
    conn: &rusqlite::Connection,
    message: &NewMessage,
) -> Result<Option<String>, StoreError> {
    if let Some(message_id) = &message.message_id {
        let found = conn
            .query_row(
                "SELECT message_id FROM messages WHERE message_id = ?1",
                [message_id],
                |row| row.get(0),
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }

    if let Some(txn_id) = &message.txn_id {
        let found = conn
            .query_row(
                "SELECT message_id FROM messages WHERE txn_id = ?1",
                [txn_id],
                |row| row.get(0),
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }

    if let Some(remote_msg_id) = &message.remote_msg_id {
        return Ok(conn
            .query_row(
                "SELECT message_id FROM messages WHERE chat_id = ?1 AND remote_msg_id = ?2",
                rusqlite::params![message.chat_id, remote_msg_id],
                |row| row.get(0),
            )
            .optional()?);
    }

    Ok(None)
}

fn insert_message(
    conn: &rusqlite::Connection,
    message_id: &str,
    message: &NewMessage,
) -> Result<(), StoreError> {
    let extra = extra_text(message.extra.as_ref())?;
    conn.execute(
        "INSERT INTO messages
             (message_id, chat_id, remote_msg_id, txn_id, sender_remote_id, is_own, sent_at,
              edited_at, kind, body, formatted, reply_to_message_id, media_id, send_state, extra)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            message_id,
            message.chat_id,
            message.remote_msg_id,
            message.txn_id,
            message.sender_remote_id,
            message.is_own,
            message.sent_at,
            message.edited_at,
            message.kind,
            message.body,
            message.formatted,
            message.reply_to_message_id,
            message.media_id,
            message.send_state,
            extra,
        ],
    )?;
    Ok(())
}

fn update_message(
    conn: &rusqlite::Connection,
    message_id: &str,
    message: &NewMessage,
) -> Result<(), StoreError> {
    let extra = extra_text(message.extra.as_ref())?;
    conn.execute(
        "UPDATE messages SET
             remote_msg_id = COALESCE(?2, remote_msg_id),
             txn_id = COALESCE(?3, txn_id),
             sender_remote_id = ?4,
             is_own = ?5,
             sent_at = ?6,
             edited_at = COALESCE(?7, edited_at),
             kind = ?8,
             body = ?9,
             formatted = ?10,
             reply_to_message_id = ?11,
             media_id = ?12,
             send_state = COALESCE(?13, send_state),
             extra = COALESCE(?14, extra)
         WHERE message_id = ?1",
        rusqlite::params![
            message_id,
            message.remote_msg_id,
            message.txn_id,
            message.sender_remote_id,
            message.is_own,
            message.sent_at,
            message.edited_at,
            message.kind,
            message.body,
            message.formatted,
            message.reply_to_message_id,
            message.media_id,
            message.send_state,
            extra,
        ],
    )?;
    Ok(())
}

fn extra_text(extra: Option<&serde_json::Value>) -> Result<Option<String>, StoreError> {
    extra
        .map(serde_json::to_string)
        .transpose()
        .map_err(StoreError::from)
}

fn message_chat(conn: &rusqlite::Connection, message_id: &str) -> Result<String, StoreError> {
    conn.query_row(
        "SELECT chat_id FROM messages WHERE message_id = ?1",
        [message_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or(StoreError::NotFound("message"))
}
