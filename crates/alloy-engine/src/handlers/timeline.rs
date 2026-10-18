use super::rooms::map_err;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::AlloyClient;
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, EventId, RoomId, SubscriptionId, TimelineItemDto,
    VectorDiffDto,
};
use std::sync::Arc;

/// Handler for timeline/messaging operations (`CommandGroup::Timeline`).
///
/// Owned by stream: `core-rooms-timeline`.
#[derive(Default)]
pub struct TimelineHandler;

impl TimelineHandler {
    pub fn new() -> Self {
        Self
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
                let client = ctx.active_client().await?;
                // Fail fast on unknown rooms instead of inside the spawned task.
                client.cached_timeline(&room_id).await.map_err(map_err)?;
                let subscription_id = spawn_subscription(ctx, client, room_id, None).await;
                Ok(EngineResponse::TimelineSubscribed { subscription_id })
            }
            EngineCommand::SubscribeThread {
                room_id,
                thread_root_event_id,
            } => {
                let client = ctx.active_client().await?;
                // Fail fast on unknown rooms or roots instead of inside the spawned task.
                client
                    .cached_thread_timeline(&room_id, &thread_root_event_id)
                    .await
                    .map_err(map_err)?;
                let subscription_id =
                    spawn_subscription(ctx, client, room_id, Some(thread_root_event_id)).await;
                Ok(EngineResponse::TimelineSubscribed { subscription_id })
            }
            EngineCommand::PaginateBackwards { room_id, count } => {
                let client = ctx.active_client().await?;
                let count = u16::try_from(count).unwrap_or(u16::MAX).max(1);
                let reached_start = client
                    .paginate_backwards(&room_id, count)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Paginated {
                    room_id,
                    reached_start,
                })
            }
            EngineCommand::SendText {
                room_id,
                body,
                markdown,
                reply_to,
                mentions,
                txn_id: _,
                in_thread_root,
            } => {
                if body.trim().is_empty() {
                    return Err(EngineError::invalid_argument("empty message"));
                }
                let client = ctx.active_client().await?;
                let txn_id = client
                    .send_text(
                        &room_id,
                        &body,
                        markdown.unwrap_or(false),
                        reply_to.as_ref(),
                        mentions.as_deref(),
                        in_thread_root.as_ref(),
                    )
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::MessageSent {
                    room_id,
                    txn_id,
                    event_id: None,
                })
            }
            EngineCommand::EditMessage {
                room_id,
                event_id,
                body,
                markdown,
            } => {
                let client = ctx.active_client().await?;
                client
                    .edit_message(&room_id, &event_id, &body, markdown.unwrap_or(false))
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::MessageEdited { room_id, event_id })
            }
            EngineCommand::RedactEvent {
                room_id,
                event_id,
                reason,
            } => {
                let client = ctx.active_client().await?;
                client
                    .redact_event(&room_id, &event_id, reason.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::EventRedacted { room_id, event_id })
            }
            EngineCommand::ToggleReaction {
                room_id,
                event_id,
                key,
            } => {
                let client = ctx.active_client().await?;
                let added = client
                    .toggle_reaction(&room_id, &event_id, &key)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::ReactionToggled {
                    room_id,
                    event_id,
                    key,
                    added,
                })
            }
            EngineCommand::RetrySend { room_id, txn_id } => {
                let client = ctx.active_client().await?;
                client
                    .retry_send(&room_id, &txn_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::SendRetried { room_id, txn_id })
            }
            EngineCommand::CancelSend { room_id, txn_id } => {
                let client = ctx.active_client().await?;
                client
                    .cancel_send(&room_id, &txn_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::SendCancelled { room_id, txn_id })
            }
            EngineCommand::MarkRead { room_id, event_id } => {
                let client = ctx.active_client().await?;
                client
                    .mark_read(&room_id, &event_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::ReadMarked { room_id })
            }
            EngineCommand::SetTyping { room_id, typing } => {
                let client = ctx.active_client().await?;
                client.set_typing(&room_id, typing).await.map_err(map_err)?;
                Ok(EngineResponse::TypingStatusSet { room_id })
            }
            EngineCommand::SendPoll {
                room_id,
                question,
                answers,
                disclosed,
                max_selections,
            } => {
                if question.trim().is_empty() {
                    return Err(EngineError::invalid_argument("empty poll question"));
                }
                if !(2..=20).contains(&answers.len()) {
                    return Err(EngineError::invalid_argument(
                        "a poll needs between 2 and 20 answers",
                    ));
                }
                if answers.iter().any(|a| a.trim().is_empty()) {
                    return Err(EngineError::invalid_argument("empty poll answer"));
                }
                if max_selections == 0 || (max_selections as usize) > answers.len() {
                    return Err(EngineError::invalid_argument(
                        "max selections must be between 1 and the number of answers",
                    ));
                }
                let client = ctx.active_client().await?;
                let txn_id = client
                    .send_poll(&room_id, &question, &answers, disclosed, max_selections)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::MessageSent {
                    room_id,
                    txn_id,
                    event_id: None,
                })
            }
            EngineCommand::RespondToPoll {
                room_id,
                poll_event_id,
                answer_ids,
            } => {
                let client = ctx.active_client().await?;
                client
                    .respond_to_poll(&room_id, &poll_event_id, &answer_ids)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::EndPoll {
                room_id,
                poll_event_id,
            } => {
                let client = ctx.active_client().await?;
                client
                    .end_poll(&room_id, &poll_event_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Timeline group",
                name
            ))),
        }
    }
}

/// Spawn a task that streams a room timeline, or one thread of it, as `TimelineDiff` events and
/// register it for cancellation. Thread diffs carry their own subscription id, so the UI keeps
/// them apart from the room timeline.
async fn spawn_subscription(
    ctx: &EngineContext,
    client: Arc<AlloyClient>,
    room_id: RoomId,
    thread_root: Option<EventId>,
) -> SubscriptionId {
    let subscription_id = SubscriptionId(uuid::Uuid::new_v4().to_string());
    let sink = ctx.sink.clone();
    let sid = subscription_id.clone();
    let rid = room_id.clone();
    let task = tokio::spawn(async move {
        let emit_sink = sink.clone();
        let emit_rid = rid.clone();
        let on_diffs = move |diffs: Vec<VectorDiffDto<TimelineItemDto>>| {
            for diff in diffs {
                emit_sink.emit(EngineEvent::TimelineDiff {
                    subscription_id: sid.clone(),
                    room_id: emit_rid.clone(),
                    diff: Box::new(diff),
                });
            }
        };
        let res = match &thread_root {
            Some(root) => client.run_thread_timeline(&rid, root, on_diffs).await,
            None => client.run_timeline(&rid, on_diffs).await,
        };
        if let Err(e) = res {
            sink.emit(EngineEvent::ToastNotice {
                message: format!("Timeline stopped: {e}"),
                level: "error".into(),
            });
        }
    });
    ctx.subscriptions
        .register(subscription_id.clone(), task.abort_handle())
        .await;
    subscription_id
}
