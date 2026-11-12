//! Small helpers shared by the command handlers and the background workers.

use std::path::Path;
use std::sync::Arc;

use alloy_protocol::{AccountId, EventId, RoomId};
use alloy_store::{
    AccountRecord, AccountState, ChatRecord, MessageKind, MessageRecord, NewMessage, NewOutboxItem,
    OutboxOp, SendState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::accounts::AccountRuntime;
use crate::context::EngineContext;
use crate::error::{EngineError, map_store};

/// Milliseconds since the Unix epoch, UTC.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// The chat row a command names, or `not_found`.
pub async fn chat_record(ctx: &EngineContext, room_id: &RoomId) -> Result<ChatRecord, EngineError> {
    ctx.store
        .get_chat(&room_id.0)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("chat '{}'", room_id.0)))
}

/// The account row a command names, or `not_found`.
pub async fn account_record(
    ctx: &EngineContext,
    account_id: &AccountId,
) -> Result<AccountRecord, EngineError> {
    ctx.store
        .get_account(&account_id.0)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("account '{}'", account_id.0)))
}

/// The live connector for an account, or `unavailable` when nothing is running for it.
pub async fn runtime_for(
    ctx: &EngineContext,
    account_id: &str,
) -> Result<Arc<AccountRuntime>, EngineError> {
    ctx.accounts
        .get(account_id)
        .await
        .ok_or_else(|| EngineError::unavailable("the account is not connected"))
}

/// The chat plus the running account that owns it: what every mutating command needs.
pub async fn chat_and_runtime(
    ctx: &EngineContext,
    room_id: &RoomId,
) -> Result<(ChatRecord, AccountRecord, Arc<AccountRuntime>), EngineError> {
    let chat = chat_record(ctx, room_id).await?;
    let account = ctx
        .store
        .get_account(&chat.account_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found("account"))?;
    if account.state == AccountState::Disabled {
        return Err(EngineError::unavailable("the account is disabled"));
    }
    let runtime = runtime_for(ctx, &chat.account_id).await?;
    Ok((chat, account, runtime))
}

/// One message of a chat, or `not_found` when the id belongs to another chat or none at all.
pub async fn message_in_chat(
    ctx: &EngineContext,
    chat_id: &str,
    event_id: &EventId,
) -> Result<MessageRecord, EngineError> {
    let record = ctx
        .store
        .get_message(&event_id.0)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("message '{}'", event_id.0)))?;
    if record.chat_id != chat_id {
        return Err(EngineError::not_found(format!(
            "message '{}' in chat '{}'",
            event_id.0, chat_id
        )));
    }
    Ok(record)
}

/// The network id of a message that has already been accepted by the network.
pub fn remote_id(record: &MessageRecord) -> Result<String, EngineError> {
    record
        .remote_msg_id
        .clone()
        .ok_or_else(|| EngineError::unavailable("the message has not reached the network yet"))
}

/// A file the outbox must upload with a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxAttachment {
    pub path: String,
    pub mime: Option<String>,
    pub file_name: Option<String>,
    /// The engine copied this file into its own directory for the upload and deletes it once the
    /// network accepted it (or refused it for good). The user's original is never touched.
    #[serde(default)]
    pub temporary: bool,
}

/// Everything needed to create one outgoing message: what the local echo shows, what the network
/// receives, and the optional attachment whose bytes the connector uploads.
#[derive(Debug, Clone)]
pub struct OutgoingDraft {
    pub kind: MessageKind,
    /// Body stored on the local echo. For an attachment without a caption this is the file name,
    /// so the sender sees a label while the upload is in flight.
    pub body: Option<String>,
    /// Body the network receives. For an attachment this is only the user's caption; the file name
    /// is never turned into message text.
    pub send_body: Option<String>,
    pub reply_to: Option<EventId>,
    pub txn_id: Option<String>,
    pub attachment: Option<OutboxAttachment>,
}

impl OutgoingDraft {
    pub fn text(body: impl Into<String>) -> Self {
        let body = body.into();
        Self {
            kind: MessageKind::Text,
            body: Some(body.clone()),
            send_body: Some(body),
            reply_to: None,
            txn_id: None,
            attachment: None,
        }
    }
}

/// Stores a local echo of an outgoing message and queues the matching outbox operation.
///
/// The row exists before the queue entry does, so the outbox worker can always resolve the message
/// it is about to send.
pub async fn queue_message(
    ctx: &EngineContext,
    chat: &ChatRecord,
    account: &AccountRecord,
    draft: OutgoingDraft,
) -> Result<String, EngineError> {
    let own = account
        .remote_user_id
        .clone()
        .ok_or_else(|| EngineError::unavailable("the account has no signed-in user yet"))?;
    let reply_remote = match &draft.reply_to {
        Some(event_id) => Some(remote_id(
            &message_in_chat(ctx, &chat.chat_id, event_id).await?,
        )?),
        None => None,
    };

    let txn_id = draft.txn_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let sent_at = now_ms();
    let record = ctx
        .store
        .upsert_message(NewMessage {
            chat_id: chat.chat_id.clone(),
            message_id: None,
            remote_msg_id: None,
            txn_id: Some(txn_id.clone()),
            sender_remote_id: own,
            is_own: true,
            sent_at,
            edited_at: None,
            kind: draft.kind,
            body: draft.body.clone(),
            formatted: None,
            reply_to_message_id: draft.reply_to.map(|event_id| event_id.0),
            media_id: None,
            send_state: Some(SendState::Queued),
            extra: None,
        })
        .await
        .map_err(map_store)?;

    ctx.store
        .update_chat_meta(
            &chat.chat_id,
            None,
            None,
            Some(record.message_id.clone()),
            Some(sent_at),
        )
        .await
        .map_err(map_store)?;

    let payload = json!({
        "body": draft.send_body,
        "reply_to": reply_remote,
        "attachment": draft.attachment,
    });
    let queued = ctx
        .store
        .enqueue_outbox(NewOutboxItem {
            txn_id: Some(txn_id.clone()),
            account_id: chat.account_id.clone(),
            chat_id: chat.chat_id.clone(),
            op: OutboxOp::Send,
            payload,
            next_attempt_at: sent_at,
            created_at: sent_at,
        })
        .await;
    if let Err(err) = queued {
        // Nothing can carry the message now; drop the echo instead of leaving a row that never
        // leaves "queued".
        let _ = ctx.store.delete_local_message(&record.message_id).await;
        return Err(map_store(err));
    }

    if let Some(runtime) = ctx.accounts.get(&chat.account_id).await {
        runtime.notify_outbox();
    }
    ctx.bus.chats();
    ctx.bus.timeline(&chat.chat_id);
    Ok(txn_id)
}

/// Queues one non-message operation (edit, delete, reaction, read, typing) for the outbox.
pub async fn enqueue_op(
    ctx: &EngineContext,
    account_id: &str,
    chat_id: &str,
    op: OutboxOp,
    payload: serde_json::Value,
) -> Result<String, EngineError> {
    let now = now_ms();
    let txn_id = ctx
        .store
        .enqueue_outbox(NewOutboxItem {
            txn_id: None,
            account_id: account_id.to_string(),
            chat_id: chat_id.to_string(),
            op,
            payload,
            next_attempt_at: now,
            created_at: now,
        })
        .await
        .map_err(map_store)?;
    if let Some(runtime) = ctx.accounts.get(account_id).await {
        runtime.notify_outbox();
    }
    Ok(txn_id)
}

/// Marks an outgoing message as definitively failed, keeping the error for the UI.
pub async fn mark_message_failed(
    ctx: &EngineContext,
    message_id: &str,
    error: &str,
) -> Result<(), EngineError> {
    let Some(record) = ctx.store.get_message(message_id).await.map_err(map_store)? else {
        return Ok(());
    };
    let mut extra = record
        .extra
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut extra {
        map.insert("send_error".to_string(), json!(error));
        map.insert("can_retry".to_string(), json!(true));
    }
    ctx.store
        .upsert_message(NewMessage {
            chat_id: record.chat_id.clone(),
            message_id: Some(record.message_id),
            remote_msg_id: record.remote_msg_id,
            txn_id: record.txn_id,
            sender_remote_id: record.sender_remote_id,
            is_own: record.is_own,
            sent_at: record.sent_at,
            edited_at: record.edited_at,
            kind: record.kind,
            body: record.body,
            formatted: record.formatted,
            reply_to_message_id: record.reply_to_message_id,
            media_id: record.media_id,
            send_state: Some(SendState::Failed),
            extra: Some(extra),
        })
        .await
        .map_err(map_store)?;
    ctx.bus.timeline(&record.chat_id);
    Ok(())
}

/// Classifies an attachment by its MIME type, the same way the connectors do.
pub fn kind_for_mime(mime: Option<&str>) -> MessageKind {
    match mime.unwrap_or_default() {
        mime if mime.starts_with("image/") => MessageKind::Image,
        mime if mime.starts_with("video/") => MessageKind::Video,
        mime if mime.starts_with("audio/") => MessageKind::Audio,
        _ => MessageKind::File,
    }
}

/// File name of a path, for the name recipients see.
pub fn file_name_of(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}
