//! Owned by stream core-rooms-timeline

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::RoomId;
use matrix_sdk::ruma::RoomId as RumaRoomId;
use matrix_sdk::ruma::UInt;
use matrix_sdk::ruma::events::AnyMessageLikeEventContent;
use matrix_sdk::ruma::events::poll::start::PollKind;
use matrix_sdk::ruma::events::poll::unstable_end::UnstablePollEndEventContent;
use matrix_sdk::ruma::events::poll::unstable_response::UnstablePollResponseEventContent;
use matrix_sdk::ruma::events::poll::unstable_start::{
    NewUnstablePollStartEventContent, UnstablePollAnswer, UnstablePollAnswers,
    UnstablePollStartContentBlock, UnstablePollStartEventContent,
};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk_ui::timeline::{Timeline, TimelineFocus};

impl AlloyClient {
    /// Open a room's timeline.
    pub async fn timeline(&self, room_id: &RoomId) -> AlloyResult<Timeline> {
        self.build_timeline(room_id, None).await
    }

    /// Build a room timeline, or one focused on a thread when `focus` is set.
    async fn build_timeline(
        &self,
        room_id: &RoomId,
        focus: Option<TimelineFocus>,
    ) -> AlloyResult<Timeline> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;

        let mut builder = matrix_sdk_ui::timeline::TimelineBuilder::new(&room);
        if let Some(focus) = focus {
            builder = builder.with_focus(focus);
        }
        let timeline = builder
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

async fn recover_local_echo_txn(
    timeline: &Timeline,
    known_txns: &std::collections::HashSet<String>,
) -> String {
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
            return t;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    format!("sent-{}", rand::random::<u64>())
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
    /// Make sure the room's encryption state is current before anything is sent, so a freshly
    /// created or just-joined encrypted room is never written to in plaintext.
    pub(crate) async fn ensure_encryption_state(&self, room_id: &RoomId) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        room.latest_encryption_state()
            .await
            .map(|_| ())
            .map_err(sdk)
    }

    /// Timeline for a room, built once and cached until sync stops / the client is dropped.
    pub async fn cached_timeline(&self, room_id: &RoomId) -> AlloyResult<Arc<Timeline>> {
        let mut map = self.timelines.lock().await;
        if let Some(t) = map.get(&room_id.0) {
            return Ok(t.clone());
        }
        let timeline = Arc::new(self.timeline(room_id).await?);
        // Load the room's members in the background so sender display names resolve for
        // every item; the SDK records failures as `TimelineDetails::Error` on the items.
        let loading = timeline.clone();
        tokio::spawn(async move {
            loading.fetch_members().await;
        });
        map.insert(room_id.0.clone(), timeline.clone());
        Ok(timeline)
    }

    /// Timeline focused on one thread, cached alongside the room timelines under a composite key
    /// (room ids start with `!`, so the `|thread|` key cannot collide with a room key).
    pub async fn cached_thread_timeline(
        &self,
        room_id: &RoomId,
        root: &alloy_protocol::EventId,
    ) -> AlloyResult<Arc<Timeline>> {
        let key = format!("{}|thread|{}", room_id.0, root.0);
        let mut map = self.timelines.lock().await;
        if let Some(t) = map.get(&key) {
            return Ok(t.clone());
        }
        let focus = TimelineFocus::Thread {
            root_event_id: event_id(root)?,
        };
        let timeline = Arc::new(self.build_timeline(room_id, Some(focus)).await?);
        map.insert(key, timeline.clone());
        Ok(timeline)
    }

    /// Drive a room timeline, calling `on_diffs` for every batch (first batch is a `Reset`
    /// with the current items). Runs until aborted; spawn and cancel to unsubscribe.
    pub async fn run_timeline(
        &self,
        room_id: &RoomId,
        on_diffs: impl FnMut(Vec<VectorDiffDto<TimelineItemDto>>) + Send,
    ) -> AlloyResult<()> {
        let timeline = self.cached_timeline(room_id).await?;
        self.drive_timeline(timeline, on_diffs).await
    }

    /// Drive one thread's timeline with the same batching as [`Self::run_timeline`].
    pub async fn run_thread_timeline(
        &self,
        room_id: &RoomId,
        root: &alloy_protocol::EventId,
        on_diffs: impl FnMut(Vec<VectorDiffDto<TimelineItemDto>>) + Send,
    ) -> AlloyResult<()> {
        let timeline = self.cached_thread_timeline(room_id, root).await?;
        self.drive_timeline(timeline, on_diffs).await
    }

    async fn drive_timeline(
        &self,
        timeline: Arc<Timeline>,
        mut on_diffs: impl FnMut(Vec<VectorDiffDto<TimelineItemDto>>) + Send,
    ) -> AlloyResult<()> {
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
    /// Returns the transaction id of the local echo. With `in_thread_root` set, the message is
    /// posted into that thread; otherwise it goes to the room timeline.
    pub async fn send_text(
        &self,
        room_id: &RoomId,
        body: &str,
        markdown: bool,
        reply_to: Option<&alloy_protocol::EventId>,
        mentions: Option<&[UserId]>,
        in_thread_root: Option<&alloy_protocol::EventId>,
    ) -> AlloyResult<String> {
        self.ensure_encryption_state(room_id).await?;
        let timeline = match in_thread_root {
            Some(root) => self.cached_thread_timeline(room_id, root).await?,
            None => self.cached_timeline(room_id).await?,
        };
        let content = build_content(body, markdown, mentions);
        let known_txns: std::collections::HashSet<String> = timeline
            .items()
            .await
            .iter()
            .filter_map(|i| i.as_event())
            .filter_map(|e| e.transaction_id().map(|t| t.to_string()))
            .collect();
        // In a threaded timeline the SDK adds the thread relation to replies, so the thread
        // root is the default parent for a thread message without an explicit reply target.
        let reply_parent = reply_to.or(in_thread_root);
        let handle = match reply_parent {
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
        Ok(recover_local_echo_txn(&timeline, &known_txns).await)
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

    /// Create a poll (MSC3381). Answers get ids `"0"`, `"1"`, … in the order given.
    ///
    /// Sent directly instead of through the timeline's send queue: the poll's local echo was not
    /// always merged with its remote echo (the poll showed up twice), so the poll appears when the
    /// server's event arrives and its event id is returned.
    pub async fn send_poll(
        &self,
        room_id: &RoomId,
        question: &str,
        answers: &[String],
        disclosed: bool,
        max_selections: u32,
    ) -> AlloyResult<alloy_protocol::EventId> {
        self.ensure_encryption_state(room_id).await?;
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;
        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;
        let fallback = std::iter::once(question.to_owned())
            .chain(
                answers
                    .iter()
                    .enumerate()
                    .map(|(i, text)| format!("{}. {text}", i + 1)),
            )
            .collect::<Vec<_>>()
            .join("\n");
        let poll_answers = answers
            .iter()
            .enumerate()
            .map(|(i, text)| UnstablePollAnswer::new(i.to_string(), text.as_str()))
            .collect::<Vec<_>>();
        let mut block = UnstablePollStartContentBlock::new(
            question,
            UnstablePollAnswers::try_from(poll_answers).map_err(sdk)?,
        );
        block.kind = if disclosed {
            PollKind::Disclosed
        } else {
            PollKind::Undisclosed
        };
        block.max_selections = UInt::from(max_selections);
        let content = UnstablePollStartEventContent::New(
            NewUnstablePollStartEventContent::plain_text(fallback, block),
        );
        let response = room
            .send(AnyMessageLikeEventContent::UnstablePollStart(content))
            .await
            .map_err(sdk)?;
        Ok(alloy_protocol::EventId(
            response.response.event_id.to_string(),
        ))
    }

    /// Submit our answer(s) to a poll, replacing any earlier response.
    pub async fn respond_to_poll(
        &self,
        room_id: &RoomId,
        poll_event_id: &alloy_protocol::EventId,
        answer_ids: &[String],
    ) -> AlloyResult<()> {
        self.ensure_encryption_state(room_id).await?;
        let timeline = self.cached_timeline(room_id).await?;
        let content =
            UnstablePollResponseEventContent::new(answer_ids.to_vec(), event_id(poll_event_id)?);
        let handle = timeline
            .send(AnyMessageLikeEventContent::UnstablePollResponse(content))
            .await
            .map_err(sdk)?;
        drop(handle);
        Ok(())
    }

    /// End a poll. Receivers only honour the end event when it comes from the poll creator.
    pub async fn end_poll(
        &self,
        room_id: &RoomId,
        poll_event_id: &alloy_protocol::EventId,
    ) -> AlloyResult<()> {
        self.ensure_encryption_state(room_id).await?;
        let timeline = self.cached_timeline(room_id).await?;
        let content =
            UnstablePollEndEventContent::new("This poll has ended.", event_id(poll_event_id)?);
        let handle = timeline
            .send(AnyMessageLikeEventContent::UnstablePollEnd(content))
            .await
            .map_err(sdk)?;
        drop(handle);
        Ok(())
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
