//! Owned by stream core-rooms-timeline

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::RoomId;
use matrix_sdk::ruma::RoomId as RumaRoomId;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk_ui::timeline::Timeline;

impl AlloyClient {
    /// Open a room's timeline.
    pub async fn timeline(&self, room_id: &RoomId) -> AlloyResult<Timeline> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;

        let timeline = matrix_sdk_ui::timeline::TimelineBuilder::new(&room)
            .build()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(timeline)
    }

    /// Send a text message to a room using send_queue for local echo.
    pub async fn send_message(
        &self,
        room_id: &RoomId,
        content: &str,
    ) -> AlloyResult<alloy_protocol::EventId> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;
        let msg_content = RoomMessageEventContent::text_plain(content);
        let response = room
            .send(msg_content)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(alloy_protocol::EventId(
            response.response.event_id.to_string(),
        ))
    }
}

use crate::mapping;
use alloy_protocol::{TimelineItemDto, UserId, VectorDiffDto};
use futures::StreamExt;
use matrix_sdk::room::edit::EditedContent;
use matrix_sdk::ruma::events::Mentions;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContentWithoutRelation;
use matrix_sdk::ruma::{OwnedEventId, OwnedTransactionId, OwnedUserId};
use matrix_sdk_ui::timeline::TimelineEventItemId;
use std::sync::Arc;

fn event_id(id: &alloy_protocol::EventId) -> AlloyResult<OwnedEventId> {
    OwnedEventId::try_from(id.0.as_str())
        .map_err(|e| AlloyError::MatrixSdk(format!("bad event id: {e}")))
}

fn sdk<E: std::fmt::Display>(e: E) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
}

fn build_content(
    body: &str,
    markdown: bool,
    mentions: Option<&[UserId]>,
) -> RoomMessageEventContentWithoutRelation {
    let content = if markdown {
        RoomMessageEventContentWithoutRelation::text_markdown(body)
    } else {
        RoomMessageEventContentWithoutRelation::text_plain(body)
    };
    match mentions {
        Some(users) if !users.is_empty() => content.add_mentions(Mentions::with_user_ids(
            users
                .iter()
                .filter_map(|u| OwnedUserId::try_from(u.0.as_str()).ok()),
        )),
        _ => content,
    }
}

impl AlloyClient {
    /// Timeline for a room, built once and cached until sync stops / the client is dropped.
    pub async fn cached_timeline(&self, room_id: &RoomId) -> AlloyResult<Arc<Timeline>> {
        let mut map = self.timelines.lock().await;
        if let Some(t) = map.get(&room_id.0) {
            return Ok(t.clone());
        }
        let timeline = Arc::new(self.timeline(room_id).await?);
        map.insert(room_id.0.clone(), timeline.clone());
        Ok(timeline)
    }

    /// Drive a room timeline, calling `on_diffs` for every batch (first batch is a `Reset`
    /// with the current items). Runs until aborted; spawn and cancel to unsubscribe.
    pub async fn run_timeline(
        &self,
        room_id: &RoomId,
        mut on_diffs: impl FnMut(Vec<VectorDiffDto<TimelineItemDto>>) + Send,
    ) -> AlloyResult<()> {
        let timeline = self.cached_timeline(room_id).await?;
        let me = self.client.user_id().map(|u| u.to_string());
        let map = move |it: &Arc<matrix_sdk_ui::timeline::TimelineItem>| {
            mapping::timeline_item_to_dto(it, me.as_deref())
        };
        let (initial, stream) = timeline.subscribe().await;
        on_diffs(vec![VectorDiffDto::Reset {
            values: initial.iter().map(&map).collect(),
        }]);
        futures::pin_mut!(stream);
        while let Some(diffs) = stream.next().await {
            on_diffs(
                diffs
                    .into_iter()
                    .map(|d| mapping::map_diff(d, &map))
                    .collect(),
            );
        }
        Ok(())
    }

    /// Load older events. Returns `true` when the start of the room was reached.
    pub async fn paginate_backwards(&self, room_id: &RoomId, count: u16) -> AlloyResult<bool> {
        self.cached_timeline(room_id)
            .await?
            .paginate_backwards(count)
            .await
            .map_err(sdk)
    }

    /// Send a text message through the send queue (local echo appears in the timeline stream).
    /// Returns the transaction id of the local echo.
    pub async fn send_text(
        &self,
        room_id: &RoomId,
        body: &str,
        markdown: bool,
        reply_to: Option<&alloy_protocol::EventId>,
        mentions: Option<&[UserId]>,
    ) -> AlloyResult<String> {
        let timeline = self.cached_timeline(room_id).await?;
        let content = build_content(body, markdown, mentions);
        let known_txns: std::collections::HashSet<String> = timeline
            .items()
            .await
            .iter()
            .filter_map(|i| i.as_event())
            .filter_map(|e| e.transaction_id().map(|t| t.to_string()))
            .collect();
        let handle = match reply_to {
            Some(parent) => timeline
                .send_reply(content, event_id(parent)?)
                .await
                .map_err(sdk)?,
            None => timeline
                .send(
                    matrix_sdk::ruma::events::AnyMessageLikeEventContent::RoomMessage(
                        content.with_relation(None),
                    ),
                )
                .await
                .map_err(sdk)?,
        };
        drop(handle);
        // The SDK keeps the transaction id private; recover it from the local echo that just
        // appeared. If the message was already confirmed by the server, no retry/cancel can
        // apply, so a synthetic id is returned.
        for _ in 0..50 {
            let found = timeline
                .items()
                .await
                .iter()
                .rev()
                .filter_map(|i| i.as_event())
                .filter(|e| e.is_own())
                .filter_map(|e| e.transaction_id().map(|t| t.to_string()))
                .find(|t| !known_txns.contains(t));
            if let Some(t) = found {
                return Ok(t);
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Ok(format!("sent-{}", rand::random::<u64>()))
    }

    /// Edit one of our own messages.
    pub async fn edit_message(
        &self,
        room_id: &RoomId,
        target: &alloy_protocol::EventId,
        body: &str,
        markdown: bool,
    ) -> AlloyResult<()> {
        let timeline = self.cached_timeline(room_id).await?;
        timeline
            .edit(
                &TimelineEventItemId::EventId(event_id(target)?),
                EditedContent::RoomMessage(build_content(body, markdown, None)),
            )
            .await
            .map_err(sdk)
    }

    /// Redact (delete) a message.
    pub async fn redact_event(
        &self,
        room_id: &RoomId,
        target: &alloy_protocol::EventId,
        reason: Option<&str>,
    ) -> AlloyResult<()> {
        let timeline = self.cached_timeline(room_id).await?;
        timeline
            .redact(&TimelineEventItemId::EventId(event_id(target)?), reason)
            .await
            .map_err(sdk)
    }

    /// Add or remove our reaction. Returns `true` if it was added.
    pub async fn toggle_reaction(
        &self,
        room_id: &RoomId,
        target: &alloy_protocol::EventId,
        key: &str,
    ) -> AlloyResult<bool> {
        let timeline = self.cached_timeline(room_id).await?;
        timeline
            .toggle_reaction(&TimelineEventItemId::EventId(event_id(target)?), key)
            .await
            .map_err(sdk)
    }

    async fn local_echo_handle(
        &self,
        room_id: &RoomId,
        txn_id: &str,
    ) -> AlloyResult<matrix_sdk::send_queue::SendHandle> {
        let timeline = self.cached_timeline(room_id).await?;
        let txn = OwnedTransactionId::from(txn_id);
        timeline
            .items()
            .await
            .iter()
            .filter_map(|i| i.as_event())
            .find(|e| e.transaction_id() == Some(&*txn))
            .and_then(|e| e.local_echo_send_handle())
            .ok_or_else(|| AlloyError::MatrixSdk(format!("no pending local echo {txn_id}")))
    }

    /// Retry a failed local echo.
    pub async fn retry_send(&self, room_id: &RoomId, txn_id: &str) -> AlloyResult<()> {
        self.local_echo_handle(room_id, txn_id)
            .await?
            .unwedge()
            .await
            .map_err(sdk)
    }

    /// Cancel an unsent local echo. Returns whether it was still cancellable.
    pub async fn cancel_send(&self, room_id: &RoomId, txn_id: &str) -> AlloyResult<bool> {
        self.local_echo_handle(room_id, txn_id)
            .await?
            .abort()
            .await
            .map_err(sdk)
    }

    /// Send a read receipt up to the given event and mark the room as read.
    pub async fn mark_read(
        &self,
        room_id: &RoomId,
        target: &alloy_protocol::EventId,
    ) -> AlloyResult<()> {
        use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
        let timeline = self.cached_timeline(room_id).await?;
        timeline
            .send_single_receipt(ReceiptType::Read, event_id(target)?)
            .await
            .map(|_| ())
            .map_err(sdk)
    }
}
