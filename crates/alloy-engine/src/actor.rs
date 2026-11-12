//! The per-account event actor: every remote change a connector reports is applied to the store
//! in observation order, and the projections are told to re-derive.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use alloy_connector::{ConnectionState, ConnectorEvent, MediaRef, RemoteChat, RemoteMessage};
use alloy_protocol::{
    AccountId, ConnectionStatus as ConnectionStatusDto, EngineEvent, RoomId, UserId,
};
use alloy_store::{
    ChatFlagsUpdate, ChatKind as StoreChatKind, ChatRecord, ChatUpsert, NewMedia, NewMessage,
    SendState,
};
use tokio::sync::mpsc;

use crate::accounts::AccountRuntime;
use crate::context::EngineContext;
use crate::convert;
use crate::error::{EngineError, map_store};
use crate::support::now_ms;

/// Drains one account's connector events until the connector drops the sender.
pub async fn run(
    ctx: EngineContext,
    runtime: Arc<AccountRuntime>,
    mut events: mpsc::Receiver<ConnectorEvent>,
) {
    let mut typing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    while let Some(event) = events.recv().await {
        if let Err(err) = apply(&ctx, &runtime, event, &mut typing).await {
            tracing::warn!(account = %runtime.account_id, error = %err.message, "connector event was not applied");
        }
    }
}

async fn apply(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    event: ConnectorEvent,
    typing: &mut BTreeMap<String, BTreeSet<String>>,
) -> Result<(), EngineError> {
    let account_id = runtime.account_id.clone();
    match event {
        ConnectorEvent::Connection { state } => apply_connection(ctx, runtime, state).await,
        ConnectorEvent::ChatUpserted { chat } => {
            apply_chat(ctx, &account_id, chat).await?;
            ctx.bus.chats();
            Ok(())
        }
        ConnectorEvent::ChatRemoved { remote_chat_id } => {
            // The store has no hard delete for a chat; archiving keeps it out of the main list
            // while preserving the history the user may still want to read.
            if let Some(chat) = ctx
                .store
                .find_chat(&account_id, &remote_chat_id)
                .await
                .map_err(map_store)?
            {
                ctx.store
                    .update_chat_flags(
                        &chat.chat_id,
                        ChatFlagsUpdate {
                            is_archived: Some(true),
                            ..ChatFlagsUpdate::default()
                        },
                    )
                    .await
                    .map_err(map_store)?;
                ctx.bus.chats();
            }
            Ok(())
        }
        ConnectorEvent::Message { message } => apply_message(ctx, &account_id, message, true).await,
        ConnectorEvent::MessageEdited {
            remote_chat_id,
            remote_msg_id,
            body,
            formatted,
            edited_at,
        } => {
            apply_edited(
                ctx,
                &account_id,
                &remote_chat_id,
                &remote_msg_id,
                body,
                formatted,
                edited_at,
            )
            .await
        }
        ConnectorEvent::MessageDeleted {
            remote_chat_id,
            remote_msg_id,
        } => apply_deleted(ctx, &account_id, &remote_chat_id, &remote_msg_id).await,
        ConnectorEvent::Reaction {
            remote_chat_id,
            remote_msg_id,
            sender_remote_id,
            emoji,
            added,
        } => {
            apply_reaction(
                ctx,
                &account_id,
                &remote_chat_id,
                &remote_msg_id,
                &sender_remote_id,
                &emoji,
                added,
            )
            .await
        }
        ConnectorEvent::ReadMarker {
            remote_chat_id,
            remote_user_id,
            up_to_remote_msg_id,
        } => {
            apply_read_marker(
                ctx,
                &account_id,
                &remote_chat_id,
                &remote_user_id,
                &up_to_remote_msg_id,
            )
            .await
        }
        ConnectorEvent::Typing {
            remote_chat_id,
            remote_user_id,
            typing: is_typing,
        } => {
            apply_typing(
                ctx,
                &account_id,
                &remote_chat_id,
                &remote_user_id,
                is_typing,
                typing,
            )
            .await
        }
        ConnectorEvent::SessionUpdated { session } => ctx
            .store
            .put_secret(&account_id, session.as_bytes())
            .await
            .map_err(map_store),
    }
}

async fn apply_connection(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    state: ConnectionState,
) -> Result<(), EngineError> {
    let account_id = AccountId(runtime.account_id.clone());
    match state {
        ConnectionState::Offline { reason } => {
            ctx.sink.emit(EngineEvent::ConnectionStatus {
                account_id,
                status: ConnectionStatusDto::Reconnecting {
                    attempt: 1,
                    next_retry_millis: 0,
                },
            });
            ctx.sink.emit(EngineEvent::ToastNotice {
                message: reason,
                level: "warning".to_string(),
            });
        }
        ConnectionState::NeedsRelogin => {
            ctx.sink.emit(EngineEvent::ConnectionStatus {
                account_id,
                status: ConnectionStatusDto::Terminated,
            });
            crate::accounts::mark_needs_relogin(ctx, &runtime.account_id).await;
        }
        state => ctx.sink.emit(EngineEvent::ConnectionStatus {
            account_id,
            status: convert::connection_status(state),
        }),
    }
    Ok(())
}

/// Stores the remote-derived fields of one chat and mirrors its unread and flag state.
pub(crate) async fn apply_chat(
    ctx: &EngineContext,
    account_id: &str,
    chat: RemoteChat,
) -> Result<(), EngineError> {
    let avatar_media_id = match &chat.avatar {
        Some(media) => Some(store_media_ref(ctx, account_id, media).await?),
        None => None,
    };
    let chat_id = ctx
        .store
        .upsert_chat(ChatUpsert {
            account_id: account_id.to_string(),
            remote_chat_id: chat.remote_chat_id.clone(),
            kind: convert::chat_kind_connector_to_store(chat.kind),
            title: Some(chat.title.clone()),
            avatar_media_id,
            last_message_id: None,
            last_activity_at: Some(chat.last_activity_at),
        })
        .await
        .map_err(map_store)?;
    let record = ctx
        .store
        .get_chat(&chat_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::internal_error("chat vanished after upsert"))?;

    let unread = i64::from(chat.unread_count);
    if record.unread_count != unread {
        ctx.store
            .set_unread(&chat_id, unread, record.mention_count)
            .await
            .map_err(map_store)?;
    }

    let mut flags = ChatFlagsUpdate::default();
    if record.is_pinned != chat.is_pinned {
        flags.is_pinned = Some(chat.is_pinned);
    }
    if record.is_archived != chat.is_archived {
        flags.is_archived = Some(chat.is_archived);
    }
    if record.is_muted != chat.is_muted {
        flags.muted_until = Some(if chat.is_muted { Some(i64::MAX) } else { None });
    }
    if flags != ChatFlagsUpdate::default() {
        ctx.store
            .update_chat_flags(&chat_id, flags)
            .await
            .map_err(map_store)?;
    }
    Ok(())
}

/// Stores one message from the network, reconciling a local echo through its `txn_id`.
///
/// `count_unread` is true for live messages and false for history backfill, which must not make a
/// chat look unread.
pub(crate) async fn apply_message(
    ctx: &EngineContext,
    account_id: &str,
    message: RemoteMessage,
    count_unread: bool,
) -> Result<(), EngineError> {
    let chat = ensure_chat(
        ctx,
        account_id,
        &message.remote_chat_id,
        StoreChatKind::Group,
    )
    .await?;
    let media_id = match &message.media {
        Some(media) => Some(store_media_ref(ctx, account_id, media).await?),
        None => None,
    };
    let reply_to_message_id = match &message.reply_to_remote_msg_id {
        Some(remote) => ctx
            .store
            .find_message_by_remote(&chat.chat_id, remote)
            .await
            .map_err(map_store)?
            .map(|record| record.message_id),
        None => None,
    };
    let existed = ctx
        .store
        .find_message_by_remote(&chat.chat_id, &message.remote_msg_id)
        .await
        .map_err(map_store)?
        .is_some();

    let record = ctx
        .store
        .upsert_message(NewMessage {
            chat_id: chat.chat_id.clone(),
            message_id: None,
            remote_msg_id: Some(message.remote_msg_id.clone()),
            txn_id: message.txn_id.clone(),
            sender_remote_id: message.sender_remote_id.clone(),
            is_own: message.is_own,
            sent_at: message.sent_at,
            edited_at: message.edited_at,
            kind: convert::message_kind_connector_to_store(message.kind),
            body: message.body.clone(),
            formatted: message.formatted.clone(),
            reply_to_message_id,
            media_id,
            send_state: Some(SendState::Sent),
            extra: None,
        })
        .await
        .map_err(map_store)?;

    // The preview follows the newest stored message, not the chat's activity clock: a chat can
    // report activity newer than the history we have, and a preview must still exist.
    let previous_preview = match &chat.last_message_id {
        Some(message_id) => ctx.store.get_message(message_id).await.map_err(map_store)?,
        None => None,
    };
    let newest = previous_preview.is_none_or(|previous| message.sent_at >= previous.sent_at);
    if newest {
        ctx.store
            .update_chat_meta(
                &chat.chat_id,
                None,
                None,
                Some(record.message_id.clone()),
                Some(message.sent_at),
            )
            .await
            .map_err(map_store)?;
    }
    if !existed && count_unread && !message.is_own {
        ctx.store
            .increment_unread(&chat.chat_id, false)
            .await
            .map_err(map_store)?;
    }

    ctx.bus.chats();
    ctx.bus.timeline(&chat.chat_id);
    Ok(())
}

async fn apply_edited(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_msg_id: &str,
    body: Option<String>,
    formatted: Option<String>,
    edited_at: i64,
) -> Result<(), EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    let Some(record) = ctx
        .store
        .find_message_by_remote(&chat.chat_id, remote_msg_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    ctx.store
        .edit_message_body(&record.message_id, body, formatted, edited_at)
        .await
        .map_err(map_store)?;
    ctx.bus.chats();
    ctx.bus.timeline(&chat.chat_id);
    Ok(())
}

async fn apply_deleted(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_msg_id: &str,
) -> Result<(), EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    let Some(record) = ctx
        .store
        .find_message_by_remote(&chat.chat_id, remote_msg_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    ctx.store
        .mark_deleted(&record.message_id, now_ms())
        .await
        .map_err(map_store)?;
    ctx.bus.chats();
    ctx.bus.timeline(&chat.chat_id);
    Ok(())
}

async fn apply_reaction(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_msg_id: &str,
    sender_remote_id: &str,
    emoji: &str,
    added: bool,
) -> Result<(), EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    let Some(record) = ctx
        .store
        .find_message_by_remote(&chat.chat_id, remote_msg_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    if added {
        ctx.store
            .set_reaction(&record.message_id, sender_remote_id, emoji, now_ms())
            .await
            .map_err(map_store)?;
    } else {
        ctx.store
            .remove_reaction(&record.message_id, sender_remote_id, emoji)
            .await
            .map_err(map_store)?;
    }
    ctx.bus.timeline(&chat.chat_id);
    Ok(())
}

async fn apply_read_marker(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_user_id: &str,
    up_to_remote_msg_id: &str,
) -> Result<(), EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    let Some(record) = ctx
        .store
        .find_message_by_remote(&chat.chat_id, up_to_remote_msg_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    ctx.store
        .set_read_marker(&chat.chat_id, remote_user_id, &record.message_id, now_ms())
        .await
        .map_err(map_store)?;
    ctx.bus.timeline(&chat.chat_id);
    Ok(())
}

async fn apply_typing(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_user_id: &str,
    typing: bool,
    sets: &mut BTreeMap<String, BTreeSet<String>>,
) -> Result<(), EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(());
    };
    let users = sets.entry(chat.chat_id.clone()).or_default();
    if typing {
        users.insert(remote_user_id.to_string());
    } else {
        users.remove(remote_user_id);
    }
    ctx.sink.emit(EngineEvent::TypingNotice {
        room_id: RoomId(chat.chat_id),
        users: users.iter().cloned().map(UserId).collect(),
    });
    Ok(())
}

/// The chat a remote id belongs to, created as a placeholder when a message arrives first.
pub(crate) async fn ensure_chat(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    kind: StoreChatKind,
) -> Result<ChatRecord, EngineError> {
    if let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    {
        return Ok(chat);
    }
    let chat_id = ctx
        .store
        .upsert_chat(ChatUpsert {
            account_id: account_id.to_string(),
            remote_chat_id: remote_chat_id.to_string(),
            kind,
            title: None,
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: None,
        })
        .await
        .map_err(map_store)?;
    ctx.store
        .get_chat(&chat_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::internal_error("chat vanished after upsert"))
}

/// Records a network media reference and returns its local row id.
async fn store_media_ref(
    ctx: &EngineContext,
    account_id: &str,
    media: &MediaRef,
) -> Result<String, EngineError> {
    ctx.store
        .insert_media(NewMedia {
            account_id: account_id.to_string(),
            remote_ref: media.remote_ref.clone(),
            mime: Some(media.mime.clone()),
            size_bytes: media
                .size_bytes
                .map(|value| value.min(i64::MAX as u64) as i64),
            width: media.width.map(i64::from),
            height: media.height.map(i64::from),
            duration_ms: media
                .duration_ms
                .map(|value| value.min(i64::MAX as u64) as i64),
            file_name: media.file_name.clone(),
        })
        .await
        .map_err(map_store)
}
