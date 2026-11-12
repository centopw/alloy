//! Row -> record mapping shared by every query.

use rusqlite::{Connection, Error, OptionalExtension, Row, types::Type};
use serde_json::Value;

use crate::error::StoreError;
use crate::model::{
    AccountRecord, ChatRecord, EventRecord, MediaRecord, MessageRecord, OutboxItem,
    ParticipantRecord, ReactionRecord, ReadMarkerRecord,
};

fn text_error(err: serde_json::Error) -> Error {
    Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
}

fn parse_json(raw: Option<String>) -> rusqlite::Result<Option<Value>> {
    match raw {
        Some(raw) => serde_json::from_str(&raw).map(Some).map_err(text_error),
        None => Ok(None),
    }
}

pub(crate) fn account(row: &Row<'_>) -> rusqlite::Result<AccountRecord> {
    Ok(AccountRecord {
        account_id: row.get("account_id")?,
        network: row.get("network")?,
        remote_user_id: row.get("remote_user_id")?,
        display_name: row.get("display_name")?,
        avatar_media_id: row.get("avatar_media_id")?,
        state: row.get("state")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(crate) fn chat(row: &Row<'_>) -> rusqlite::Result<ChatRecord> {
    let local_tags = match row.get::<_, Option<String>>("local_tags")? {
        Some(raw) => serde_json::from_str(&raw).map_err(text_error)?,
        None => Vec::new(),
    };
    Ok(ChatRecord {
        chat_id: row.get("chat_id")?,
        account_id: row.get("account_id")?,
        remote_chat_id: row.get("remote_chat_id")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        avatar_media_id: row.get("avatar_media_id")?,
        last_message_id: row.get("last_message_id")?,
        last_activity_at: row.get("last_activity_at")?,
        unread_count: row.get("unread_count")?,
        mention_count: row.get("mention_count")?,
        is_pinned: row.get("is_pinned")?,
        is_archived: row.get("is_archived")?,
        is_muted: row.get("is_muted")?,
        muted_until: row.get("muted_until")?,
        is_marked_unread: row.get("is_marked_unread")?,
        draft: row.get("draft")?,
        local_tags,
        note: row.get("note")?,
    })
}

pub(crate) fn message(row: &Row<'_>) -> rusqlite::Result<MessageRecord> {
    let extra = parse_json(row.get("extra")?)?;
    Ok(MessageRecord {
        message_id: row.get("message_id")?,
        chat_id: row.get("chat_id")?,
        remote_msg_id: row.get("remote_msg_id")?,
        txn_id: row.get("txn_id")?,
        sender_remote_id: row.get("sender_remote_id")?,
        is_own: row.get("is_own")?,
        sent_at: row.get("sent_at")?,
        edited_at: row.get("edited_at")?,
        deleted_at: row.get("deleted_at")?,
        kind: row.get("kind")?,
        body: row.get("body")?,
        formatted: row.get("formatted")?,
        reply_to_message_id: row.get("reply_to_message_id")?,
        media_id: row.get("media_id")?,
        send_state: row.get("send_state")?,
        extra,
    })
}

pub(crate) fn outbox(row: &Row<'_>) -> rusqlite::Result<OutboxItem> {
    let payload: String = row.get("payload")?;
    Ok(OutboxItem {
        txn_id: row.get("txn_id")?,
        account_id: row.get("account_id")?,
        chat_id: row.get("chat_id")?,
        op: row.get("op")?,
        payload: serde_json::from_str(&payload).map_err(text_error)?,
        state: row.get("state")?,
        attempts: row.get("attempts")?,
        next_attempt_at: row.get("next_attempt_at")?,
        last_error: row.get("last_error")?,
        created_at: row.get("created_at")?,
    })
}

pub(crate) fn event(row: &Row<'_>) -> rusqlite::Result<EventRecord> {
    let payload: String = row.get("payload")?;
    Ok(EventRecord {
        seq: row.get("seq")?,
        account_id: row.get("account_id")?,
        chat_id: row.get("chat_id")?,
        kind: row.get("kind")?,
        payload: serde_json::from_str(&payload).map_err(text_error)?,
        created_at: row.get("created_at")?,
    })
}

pub(crate) fn participant(row: &Row<'_>) -> rusqlite::Result<ParticipantRecord> {
    Ok(ParticipantRecord {
        chat_id: row.get("chat_id")?,
        remote_user_id: row.get("remote_user_id")?,
        display_name: row.get("display_name")?,
        avatar_media_id: row.get("avatar_media_id")?,
        role: row.get("role")?,
    })
}

pub(crate) fn media(row: &Row<'_>) -> rusqlite::Result<MediaRecord> {
    Ok(MediaRecord {
        media_id: row.get("media_id")?,
        account_id: row.get("account_id")?,
        remote_ref: row.get("remote_ref")?,
        mime: row.get("mime")?,
        size_bytes: row.get("size_bytes")?,
        width: row.get("width")?,
        height: row.get("height")?,
        duration_ms: row.get("duration_ms")?,
        file_name: row.get("file_name")?,
        local_path: row.get("local_path")?,
        sha256: row.get("sha256")?,
        fetched_at: row.get("fetched_at")?,
    })
}

pub(crate) fn reaction(row: &Row<'_>) -> rusqlite::Result<ReactionRecord> {
    Ok(ReactionRecord {
        message_id: row.get("message_id")?,
        sender_remote_id: row.get("sender_remote_id")?,
        emoji: row.get("emoji")?,
        reacted_at: row.get("reacted_at")?,
    })
}

pub(crate) fn read_marker(row: &Row<'_>) -> rusqlite::Result<ReadMarkerRecord> {
    Ok(ReadMarkerRecord {
        chat_id: row.get("chat_id")?,
        remote_user_id: row.get("remote_user_id")?,
        up_to_message_id: row.get("up_to_message_id")?,
        read_at: row.get("read_at")?,
    })
}

pub(crate) fn chat_by_id(conn: &Connection, chat_id: &str) -> Result<ChatRecord, StoreError> {
    conn.query_row("SELECT * FROM chats WHERE chat_id = ?1", [chat_id], chat)
        .optional()?
        .ok_or(StoreError::NotFound("chat"))
}

pub(crate) fn message_by_id(
    conn: &Connection,
    message_id: &str,
) -> Result<MessageRecord, StoreError> {
    conn.query_row(
        "SELECT * FROM messages WHERE message_id = ?1",
        [message_id],
        message,
    )
    .optional()?
    .ok_or(StoreError::NotFound("message"))
}

pub(crate) fn account_for_chat(conn: &Connection, chat_id: &str) -> Result<String, StoreError> {
    conn.query_row(
        "SELECT account_id FROM chats WHERE chat_id = ?1",
        [chat_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or(StoreError::NotFound("chat"))
}
