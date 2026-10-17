use super::rooms::map_err;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, SubscriptionId};

/// Handler for timeline/messaging operations (`CommandGroup::Timeline`).
///
/// Owned by stream: `core-rooms-timeline`. Attachments belong to `core-media`.
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
                let subscription_id = SubscriptionId(uuid::Uuid::new_v4().to_string());
                let sink = ctx.sink.clone();
                let sid = subscription_id.clone();
                let rid = room_id.clone();
                let task = tokio::spawn(async move {
                    let res = client
                        .run_timeline(&rid, |diffs| {
                            for diff in diffs {
                                sink.emit(EngineEvent::TimelineDiff {
                                    subscription_id: sid.clone(),
                                    room_id: rid.clone(),
                                    diff: Box::new(diff),
                                });
                            }
                        })
                        .await;
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
            EngineCommand::SendAttachment { .. } => Err(EngineError::not_implemented(&name)),
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Timeline group",
                name
            ))),
        }
    }
}
