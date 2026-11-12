//! Timeline commands: subscriptions, pagination, sends and message-level mutations.

use alloy_protocol::{AccountId, EngineCommand, EngineResponse, EventId, ForwardFailure, RoomId};
use alloy_store::{MessageKind as StoreMessageKind, OutboxOp, SendState, StoreError};

use crate::context::EngineContext;
use crate::error::{EngineError, map_store};
use crate::projection;
use crate::router::{CommandGroup, GroupHandler, command_name};
use crate::support::{self, OutgoingDraft};

/// Handler for timeline commands (`CommandGroup::Timeline`).
pub struct TimelineHandler;

impl TimelineHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TimelineHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl GroupHandler for TimelineHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Timeline
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::SubscribeTimeline { room_id } => {
                let subscription_id = projection::subscribe_timeline(ctx, &room_id).await?;
                Ok(EngineResponse::TimelineSubscribed { subscription_id })
            }
            EngineCommand::CloseTimeline { room_id } => {
                if let Some(window) = ctx.timelines.remove(&room_id.0).await {
                    ctx.subscriptions.cancel(&window.subscription_id).await;
                }
                Ok(EngineResponse::TimelineClosed)
            }
            EngineCommand::PaginateBackwards { room_id, count } => {
                let reached_start = projection::paginate_backwards(ctx, &room_id, count).await?;
                Ok(EngineResponse::Paginated { reached_start })
            }
            EngineCommand::SendText {
                room_id,
                body,
                reply_to,
                txn_id,
            } => send_text(ctx, &room_id, body, reply_to, txn_id).await,
            EngineCommand::EditMessage {
                room_id,
                event_id,
                body,
            } => edit_message(ctx, &room_id, &event_id, body).await,
            EngineCommand::DeleteMessage {
                room_id,
                event_id,
                for_everyone,
            } => delete_message(ctx, &room_id, &event_id, for_everyone).await,
            EngineCommand::ToggleReaction {
                room_id,
                event_id,
                key,
            } => toggle_reaction(ctx, &room_id, &event_id, key).await,
            EngineCommand::ForwardEvents {
                room_id,
                event_ids,
                targets,
            } => forward_events(ctx, &room_id, event_ids, targets).await,
            EngineCommand::RetrySend { room_id, txn_id } => {
                retry_send(ctx, &room_id, &txn_id).await
            }
            EngineCommand::CancelSend { room_id, txn_id } => {
                cancel_send(ctx, &room_id, &txn_id).await
            }
            EngineCommand::MarkRead { room_id, event_id } => {
                mark_read(ctx, &room_id, &event_id).await
            }
            EngineCommand::SetTyping { room_id, typing } => set_typing(ctx, &room_id, typing).await,
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to the Timeline group",
                name
            ))),
        }
    }
}

async fn send_text(
    ctx: &EngineContext,
    room_id: &RoomId,
    body: String,
    reply_to: Option<EventId>,
    txn_id: Option<String>,
) -> Result<EngineResponse, EngineError> {
    let (chat, account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if body.trim().is_empty() {
        return Err(EngineError::invalid_argument("the message body is empty"));
    }
    if reply_to.is_some() && !runtime.capabilities.replies {
        return Err(EngineError::not_supported("replies"));
    }
    let txn_id = support::queue_message(
        ctx,
        &chat,
        &account,
        OutgoingDraft {
            kind: StoreMessageKind::Text,
            body: Some(body.clone()),
            send_body: Some(body),
            reply_to,
            txn_id,
            attachment: None,
        },
    )
    .await?;
    Ok(EngineResponse::MessageQueued { txn_id })
}

async fn edit_message(
    ctx: &EngineContext,
    room_id: &RoomId,
    event_id: &EventId,
    body: String,
) -> Result<EngineResponse, EngineError> {
    let (chat, _account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if !runtime.capabilities.edit {
        return Err(EngineError::not_supported("edit"));
    }
    if body.trim().is_empty() {
        return Err(EngineError::invalid_argument("the edited message is empty"));
    }
    let message = support::message_in_chat(ctx, &chat.chat_id, event_id).await?;
    if !message.is_own {
        return Err(EngineError::forbidden(
            "only your own messages can be edited",
        ));
    }
    support::remote_id(&message)?;
    if let Some(window) = runtime.capabilities.edit_window_secs {
        let age = support::now_ms().saturating_sub(message.sent_at);
        if age > i64::from(window).saturating_mul(1000) {
            return Err(EngineError::not_supported("edit_window"));
        }
    }
    support::enqueue_op(
        ctx,
        &chat.account_id,
        &chat.chat_id,
        OutboxOp::Edit,
        serde_json::json!({ "event_id": message.message_id, "body": body }),
    )
    .await?;
    Ok(EngineResponse::MessageEdited)
}

async fn delete_message(
    ctx: &EngineContext,
    room_id: &RoomId,
    event_id: &EventId,
    for_everyone: bool,
) -> Result<EngineResponse, EngineError> {
    let (chat, _account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if for_everyone && !runtime.capabilities.delete_for_everyone {
        return Err(EngineError::not_supported("delete_for_everyone"));
    }
    let message = support::message_in_chat(ctx, &chat.chat_id, event_id).await?;
    support::remote_id(&message)?;
    ctx.store
        .mark_deleted(&message.message_id, support::now_ms())
        .await
        .map_err(map_store)?;
    support::enqueue_op(
        ctx,
        &chat.account_id,
        &chat.chat_id,
        OutboxOp::Delete,
        serde_json::json!({ "event_id": message.message_id, "for_everyone": for_everyone }),
    )
    .await?;
    ctx.bus.timeline(&chat.chat_id);
    Ok(EngineResponse::MessageDeleted)
}

async fn toggle_reaction(
    ctx: &EngineContext,
    room_id: &RoomId,
    event_id: &EventId,
    key: String,
) -> Result<EngineResponse, EngineError> {
    let (chat, _account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if !runtime.capabilities.reactions {
        return Err(EngineError::not_supported("reactions"));
    }
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(EngineError::invalid_argument("the reaction is empty"));
    }
    let message = support::message_in_chat(ctx, &chat.chat_id, event_id).await?;
    let own = runtime
        .remote_user_id
        .clone()
        .ok_or_else(|| EngineError::unavailable("the account has no signed-in user yet"))?;

    let stored = ctx
        .store
        .list_reactions(std::slice::from_ref(&message.message_id))
        .await
        .map_err(map_store)?;
    let active = stored
        .iter()
        .any(|reaction| reaction.emoji == key && reaction.sender_remote_id == own);

    if active {
        ctx.store
            .remove_reaction(&message.message_id, &own, &key)
            .await
            .map_err(map_store)?;
    } else {
        ctx.store
            .set_reaction(&message.message_id, &own, &key, support::now_ms())
            .await
            .map_err(map_store)?;
    }
    ctx.bus.timeline(&chat.chat_id);

    support::enqueue_op(
        ctx,
        &chat.account_id,
        &chat.chat_id,
        if active {
            OutboxOp::Unreact
        } else {
            OutboxOp::React
        },
        serde_json::json!({ "event_id": message.message_id, "key": key }),
    )
    .await?;
    Ok(EngineResponse::ReactionToggled)
}

async fn forward_events(
    ctx: &EngineContext,
    room_id: &RoomId,
    event_ids: Vec<EventId>,
    targets: Vec<RoomId>,
) -> Result<EngineResponse, EngineError> {
    let source = support::chat_record(ctx, room_id).await?;
    let mut bodies = Vec::with_capacity(event_ids.len());
    for event_id in &event_ids {
        let message = support::message_in_chat(ctx, &source.chat_id, event_id).await?;
        let body = message
            .body
            .clone()
            .ok_or_else(|| EngineError::invalid_argument("a forwarded message has no text body"))?;
        bodies.push(body);
    }

    let mut failures = Vec::new();
    for target in targets {
        if let Err(err) = forward_to(ctx, &bodies, &target).await {
            failures.push(ForwardFailure {
                room_id: target,
                error: err.message,
            });
        }
    }
    Ok(EngineResponse::EventsForwarded { failures })
}

async fn forward_to(
    ctx: &EngineContext,
    bodies: &[String],
    target: &RoomId,
) -> Result<(), EngineError> {
    let chat = support::chat_record(ctx, target).await?;
    let account = support::account_record(ctx, &AccountId(chat.account_id.clone())).await?;
    support::runtime_for(ctx, &chat.account_id).await?;
    for body in bodies {
        support::queue_message(ctx, &chat, &account, OutgoingDraft::text(body.clone())).await?;
    }
    Ok(())
}

async fn retry_send(
    ctx: &EngineContext,
    room_id: &RoomId,
    txn_id: &str,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    let message = ctx
        .store
        .find_message_by_txn(txn_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("transaction '{txn_id}'")))?;
    if message.chat_id != chat.chat_id {
        return Err(EngineError::not_found(format!(
            "transaction '{txn_id}' in this chat"
        )));
    }

    match message.send_state {
        Some(SendState::Failed) => {
            ctx.store
                .set_send_state(&message.message_id, SendState::Queued, None, None)
                .await
                .map_err(map_store)?;
            match ctx
                .store
                .fail_outbox(txn_id, "retry requested", support::now_ms())
                .await
            {
                Ok(()) | Err(StoreError::NotFound(_)) => {}
                Err(err) => return Err(map_store(err)),
            }
            if let Some(runtime) = ctx.accounts.get(&chat.account_id).await {
                runtime.notify_outbox();
            }
            ctx.bus.timeline(&chat.chat_id);
            Ok(EngineResponse::SendRetried)
        }
        Some(SendState::Sent) | Some(SendState::Delivered) | Some(SendState::Read) => Err(
            EngineError::invalid_argument("the message was already sent"),
        ),
        _ => Ok(EngineResponse::SendRetried),
    }
}

async fn cancel_send(
    ctx: &EngineContext,
    room_id: &RoomId,
    txn_id: &str,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    let message = ctx
        .store
        .find_message_by_txn(txn_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("transaction '{txn_id}'")))?;
    if message.chat_id != chat.chat_id {
        return Err(EngineError::not_found(format!(
            "transaction '{txn_id}' in this chat"
        )));
    }
    if matches!(
        message.send_state,
        Some(SendState::Sent) | Some(SendState::Delivered) | Some(SendState::Read)
    ) {
        return Err(EngineError::invalid_argument(
            "the message was already sent",
        ));
    }

    match ctx.store.delete_outbox(txn_id).await {
        Ok(()) | Err(StoreError::NotFound(_)) => {}
        Err(err) => return Err(map_store(err)),
    }
    ctx.store
        .delete_local_message(&message.message_id)
        .await
        .map_err(map_store)?;
    ctx.bus.timeline(&chat.chat_id);
    ctx.bus.chats();
    Ok(EngineResponse::SendCancelled)
}

async fn mark_read(
    ctx: &EngineContext,
    room_id: &RoomId,
    event_id: &EventId,
) -> Result<EngineResponse, EngineError> {
    let (chat, _account, _runtime) = support::chat_and_runtime(ctx, room_id).await?;
    let message = support::message_in_chat(ctx, &chat.chat_id, event_id).await?;
    support::remote_id(&message)?;
    ctx.store
        .set_unread(&chat.chat_id, 0, 0)
        .await
        .map_err(map_store)?;
    support::enqueue_op(
        ctx,
        &chat.account_id,
        &chat.chat_id,
        OutboxOp::Read,
        serde_json::json!({ "event_id": message.message_id }),
    )
    .await?;
    ctx.bus.chats();
    Ok(EngineResponse::ReadMarked)
}

async fn set_typing(
    ctx: &EngineContext,
    room_id: &RoomId,
    typing: bool,
) -> Result<EngineResponse, EngineError> {
    let (chat, _account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if !runtime.capabilities.typing {
        return Err(EngineError::not_supported("typing"));
    }
    support::enqueue_op(
        ctx,
        &chat.account_id,
        &chat.chat_id,
        OutboxOp::Typing,
        serde_json::json!({ "typing": typing }),
    )
    .await?;
    Ok(EngineResponse::Typing)
}
