//! The per-account event actor: every remote change a connector reports is applied to the store
//! in observation order, and the projections are told to re-derive.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use alloy_connector::{
    ConnectionState, ConnectorError, ConnectorEvent, MediaRef, RemoteChat, RemoteMessage,
    RemoteUser,
};
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
use crate::error::{EngineError, map_connector, map_store};
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
            let forum = chat.is_forum.then(|| chat.remote_chat_id.clone());
            apply_chat(ctx, &account_id, chat).await?;
            let topics = match forum {
                Some(forum) if runtime.capabilities.topics => {
                    apply_topics(ctx, runtime, &forum).await
                }
                _ => Ok(()),
            };
            ctx.bus.chats();
            topics
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
        ConnectorEvent::ChatPinned {
            remote_chat_id,
            pinned,
        } => {
            if let Some(chat) = ctx
                .store
                .find_chat(&account_id, &remote_chat_id)
                .await
                .map_err(map_store)?
                && chat.is_pinned != pinned
            {
                ctx.store
                    .update_chat_flags(
                        &chat.chat_id,
                        ChatFlagsUpdate {
                            is_pinned: Some(pinned),
                            ..ChatFlagsUpdate::default()
                        },
                    )
                    .await
                    .map_err(map_store)?;
                ctx.bus.chats();
            }
            Ok(())
        }
        ConnectorEvent::PinnedChatsReordered { remote_chat_ids } => {
            // Only unarchived chats the store knows can be ordered; the UI's pinned list leaves
            // archived chats out, so the network's list must not unpin them. A chat the list names
            // before its upsert arrives is picked up by the next `ChatUpserted` carrying its `pin_order`.
            let mut chat_ids = Vec::with_capacity(remote_chat_ids.len());
            for remote_chat_id in &remote_chat_ids {
                if let Some(chat) = ctx
                    .store
                    .find_chat(&account_id, remote_chat_id)
                    .await
                    .map_err(map_store)?
                    && !chat.is_archived
                {
                    chat_ids.push(chat.chat_id);
                }
            }
            ctx.store
                .set_pinned_chats(&account_id, chat_ids)
                .await
                .map_err(map_store)?;
            ctx.bus.chats();
            Ok(())
        }
        ConnectorEvent::MessagePinned {
            remote_chat_id,
            remote_msg_id,
            pinned,
        } => apply_message_pinned(ctx, &account_id, &remote_chat_id, &remote_msg_id, pinned).await,
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
    if matches!(state, ConnectionState::NeedsRelogin) {
        runtime.mark_connection_live();
        ctx.sink.emit(EngineEvent::ConnectionStatus {
            account_id,
            status: ConnectionStatusDto::Terminated,
        });
        crate::accounts::mark_needs_relogin(ctx, &runtime.account_id).await;
        return Ok(());
    }
    // A connector retries on its own and repeats the offline report for every failed attempt, so
    // the warning marks the start of an outage: one blip is one notice, and a return to Online
    // arms the next one.
    let (notify, reason) = match &state {
        ConnectionState::Offline { reason, .. } => {
            (runtime.mark_connection_lost(), Some(reason.clone()))
        }
        _ => {
            runtime.mark_connection_live();
            (false, None)
        }
    };
    ctx.sink.emit(EngineEvent::ConnectionStatus {
        account_id,
        status: convert::connection_status(state),
    });
    if let (true, Some(message)) = (notify, reason) {
        ctx.sink.emit(EngineEvent::ToastNotice {
            message,
            level: "warning".to_string(),
        });
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
    // A topic is not pinned or archived on its own: the forum it belongs to carries that state,
    // and the UI nests the topic under it.
    let is_topic = chat.parent_remote_chat_id.is_some();
    let is_pinned = !is_topic && chat.is_pinned;
    let is_archived = !is_topic && chat.is_archived;
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
            parent_remote_chat_id: chat.parent_remote_chat_id.clone(),
            is_forum: chat.is_forum,
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
    // A pin the network placed with its position keeps that position. Writing it through
    // `update_chat_flags` would move it to the top and shift every other pinned chat.
    match is_pinned.then_some(chat.pin_order).flatten() {
        Some(order) => {
            let order = i64::from(order);
            if !record.is_pinned || record.pin_order != Some(order) {
                ctx.store
                    .set_chat_pin_state(&chat_id, Some(order))
                    .await
                    .map_err(map_store)?;
            }
        }
        None => {
            if record.is_pinned != is_pinned {
                flags.is_pinned = Some(is_pinned);
            }
        }
    }
    if record.is_archived != is_archived {
        flags.is_archived = Some(is_archived);
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

/// Stores every topic of a forum through the same path as any chat, so each topic becomes its own
/// room. A connector without topics for this chat answers `NotSupported`, which is not a failure.
pub(crate) async fn apply_topics(
    ctx: &EngineContext,
    runtime: &AccountRuntime,
    forum_remote_id: &str,
) -> Result<(), EngineError> {
    let topics = match runtime.connector.list_topics(forum_remote_id).await {
        Ok(topics) => topics,
        Err(ConnectorError::NotSupported { .. }) => return Ok(()),
        Err(err) => return Err(map_connector(err)),
    };
    for topic in topics {
        apply_chat(ctx, &runtime.account_id, topic).await?;
    }
    Ok(())
}

/// Mirrors the pin state of one message as the network reports it.
async fn apply_message_pinned(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_msg_id: &str,
    pinned: bool,
) -> Result<(), EngineError> {
    let Some(chat) = resolve_message_chat(ctx, account_id, remote_chat_id, remote_msg_id).await?
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
    if record.is_pinned == pinned {
        return Ok(());
    }
    ctx.store
        .set_message_pinned(&record.message_id, pinned)
        .await
        .map_err(map_store)?;
    ctx.bus.timeline(&chat.chat_id);
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

    if let Some(sender) = &message.sender
        && let Err(err) = remember_sender(ctx, account_id, &chat.chat_id, sender).await
    {
        tracing::warn!(chat = %chat.chat_id, error = %err.message, "failed to store a sender");
    }
    // A network that keeps message pins is authoritative in both directions: a message unpinned
    // there while this device was offline must come back unpinned. A network without them keeps
    // device-local pins, so an unpinned arrival must not clear one made here.
    let network_pins = ctx
        .accounts
        .get(account_id)
        .await
        .is_some_and(|runtime| runtime.capabilities.pin_messages);
    if network_pins {
        if message.pinned != record.is_pinned {
            ctx.store
                .set_message_pinned(&record.message_id, message.pinned)
                .await
                .map_err(map_store)?;
        }
    } else if message.pinned && !record.is_pinned {
        ctx.store
            .set_message_pinned(&record.message_id, true)
            .await
            .map_err(map_store)?;
    }

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

/// Stores the name and picture a message carried for its sender, so the timeline can name
/// someone the member list never reported (it is capped, and people leave). The role is left
/// alone: only the member list knows it.
async fn remember_sender(
    ctx: &EngineContext,
    account_id: &str,
    chat_id: &str,
    sender: &RemoteUser,
) -> Result<(), EngineError> {
    let avatar = match &sender.avatar {
        Some(media) => Some(store_media_ref(ctx, account_id, media).await?),
        None => None,
    };
    // Most messages come from someone already stored as they are now; skip the write then.
    let known = ctx
        .store
        .get_participant(chat_id, &sender.remote_user_id)
        .await
        .map_err(map_store)?;
    if known.is_some_and(|known| {
        known.display_name.as_deref() == Some(sender.display_name.as_str())
            && (avatar.is_none() || known.avatar_media_id == avatar)
    }) {
        return Ok(());
    }
    ctx.store
        .upsert_participant(
            chat_id,
            &sender.remote_user_id,
            Some(sender.display_name.clone()),
            avatar,
            None,
        )
        .await
        .map_err(map_store)?;
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
    let Some(chat) = resolve_message_chat(ctx, account_id, remote_chat_id, remote_msg_id).await?
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
    let Some(chat) = resolve_message_chat(ctx, account_id, remote_chat_id, remote_msg_id).await?
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
    let Some(chat) = resolve_message_chat(ctx, account_id, remote_chat_id, remote_msg_id).await?
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
    let Some(chat) =
        resolve_message_chat(ctx, account_id, remote_chat_id, up_to_remote_msg_id).await?
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

/// The chat an event naming `remote_chat_id` and `remote_msg_id` applies to.
///
/// A plain chat is its own answer. A forum holds no messages: Telegram names its deletes, pins and
/// read markers by the forum id alone, so the topic that holds the message is the answer. `None`
/// means no topic stores the message yet, and the event is a quiet no-op.
async fn resolve_message_chat(
    ctx: &EngineContext,
    account_id: &str,
    remote_chat_id: &str,
    remote_msg_id: &str,
) -> Result<Option<ChatRecord>, EngineError> {
    let Some(chat) = ctx
        .store
        .find_chat(account_id, remote_chat_id)
        .await
        .map_err(map_store)?
    else {
        return Ok(None);
    };
    if !chat.is_forum {
        return Ok(Some(chat));
    }
    let topic = ctx
        .store
        .find_topic_message(account_id, remote_chat_id, remote_msg_id)
        .await
        .map_err(map_store)?
        .map(|(topic, _)| topic);
    Ok(topic)
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
            parent_remote_chat_id: None,
            is_forum: false,
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
pub(crate) async fn store_media_ref(
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
