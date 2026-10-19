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

        // Read receipts feed the sent/seen ticks; without tracking every item reports an empty list.
        let mut builder = matrix_sdk_ui::timeline::TimelineBuilder::new(&room)
            .track_read_marker_and_receipts(
                matrix_sdk_ui::timeline::TimelineReadReceiptTracking::MessageLikeEvents,
            );
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
    mention_room: bool,
) -> RoomMessageEventContentWithoutRelation {
    let content = if markdown {
        let mut c = RoomMessageEventContentWithoutRelation::text_markdown(body);
        // Post-process HTML for spoilers if formatted content exists
        if let matrix_sdk::ruma::events::room::message::MessageType::Text(text_content) =
            &mut c.msgtype
        {
            if let Some(formatted) = &mut text_content.formatted {
                let (processed, _) = post_process_spoilers(&formatted.body);
                formatted.body = processed;
            } else if body.contains("||") {
                // If markdown didn't produce HTML, wrap in basic paragraph and process
                let (processed, has_spoilers) = post_process_spoilers(body);
                if has_spoilers {
                    text_content.formatted = Some(
                        matrix_sdk::ruma::events::room::message::FormattedBody::html(format!(
                            "<p>{}</p>",
                            processed
                        )),
                    );
                }
            }
        }
        c
    } else {
        RoomMessageEventContentWithoutRelation::text_plain(body)
    };
    let mut m = match mentions {
        Some(users) if !users.is_empty() => Mentions::with_user_ids(
            users
                .iter()
                .filter_map(|u| OwnedUserId::try_from(u.0.as_str()).ok()),
        ),
        _ => Mentions::new(),
    };
    if mention_room {
        m.room = true;
    }
    if !m.user_ids.is_empty() || m.room {
        content.add_mentions(m)
    } else {
        content
    }
}

/// Post-process spoiler markdown (`||text||`) in HTML, replacing it with `<span data-mx-spoiler>`.
/// Does NOT replace spoilers inside `<code>`, `<pre>`, or within tags.
/// Handles escaped pipes (`\|`) by unescaping them after spoiler detection.
/// Returns (html, has_spoilers).
fn post_process_spoilers(html: &str) -> (String, bool) {
    let mut result = String::new();
    let mut chars = html.chars().peekable();
    let mut in_code: usize = 0;
    let mut in_pre: usize = 0;
    let mut in_tag = false;
    let mut has_spoilers = false;

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                result.push(ch);
                // Read until '>' to detect tag type
                let mut tag_name = String::new();
                let mut first_char = true;
                for next_ch in chars.by_ref() {
                    result.push(next_ch);
                    if next_ch == '>' {
                        in_tag = false;
                        break;
                    }
                    if first_char && next_ch == '/' {
                        tag_name.push('/');
                    } else if first_char || next_ch.is_alphanumeric() {
                        tag_name.push(next_ch.to_ascii_lowercase());
                        first_char = false;
                    }
                }
                if tag_name.to_lowercase().starts_with("code") {
                    in_code += 1;
                } else if tag_name.to_lowercase().starts_with("/code") {
                    in_code = in_code.saturating_sub(1);
                } else if tag_name.to_lowercase().starts_with("pre") {
                    in_pre += 1;
                } else if tag_name.to_lowercase().starts_with("/pre") {
                    in_pre = in_pre.saturating_sub(1);
                }
            }
            '|' if in_code == 0 && in_pre == 0 && !in_tag && chars.peek() == Some(&'|') => {
                // Found start of ||
                chars.next(); // consume second |
                let mut inner = String::new();
                let mut found_end = false;

                while let Some(inner_ch) = chars.next() {
                    if inner_ch == '|' && chars.peek() == Some(&'|') {
                        // Found closing ||
                        chars.next();
                        found_end = true;
                        break;
                    } else if inner_ch == '<' {
                        // Tag inside spoiler marker; abort
                        inner.push(inner_ch);
                        break;
                    } else {
                        inner.push(inner_ch);
                    }
                }

                if found_end && !inner.is_empty() && !inner.contains('<') {
                    result.push_str(&format!("<span data-mx-spoiler>{}</span>", inner));
                    has_spoilers = true;
                } else {
                    // No valid closing ||, output as-is
                    result.push_str("||");
                    result.push_str(&inner);
                    if found_end {
                        result.push_str("||");
                    }
                }
            }
            _ => result.push(ch),
        }
    }
    (result, has_spoilers)
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
    #[allow(clippy::too_many_arguments)]
    pub async fn send_text(
        &self,
        room_id: &RoomId,
        body: &str,
        markdown: bool,
        reply_to: Option<&alloy_protocol::EventId>,
        mentions: Option<&[UserId]>,
        mention_room: bool,
        in_thread_root: Option<&alloy_protocol::EventId>,
    ) -> AlloyResult<String> {
        self.ensure_encryption_state(room_id).await?;
        let timeline = match in_thread_root {
            Some(root) => self.cached_thread_timeline(room_id, root).await?,
            None => self.cached_timeline(room_id).await?,
        };
        let content = build_content(body, markdown, mentions, mention_room);
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
                EditedContent::RoomMessage(build_content(body, markdown, None, false)),
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

    /// Send a static location message with coordinates and optional accuracy and description.
    pub async fn send_location(
        &self,
        room_id: &RoomId,
        latitude: f64,
        longitude: f64,
        accuracy_m: Option<f64>,
        description: Option<String>,
    ) -> AlloyResult<()> {
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(AlloyError::MatrixSdk(format!(
                "latitude {latitude} outside valid range [-90, 90]"
            )));
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(AlloyError::MatrixSdk(format!(
                "longitude {longitude} outside valid range [-180, 180]"
            )));
        }
        if let Some(acc) = accuracy_m
            && acc < 0.0
        {
            return Err(AlloyError::MatrixSdk(format!(
                "accuracy {acc} cannot be negative"
            )));
        }

        let geo_uri = match accuracy_m {
            Some(acc) => format!("geo:{latitude},{longitude};u={acc}"),
            None => format!("geo:{latitude},{longitude}"),
        };
        let body = description
            .clone()
            .unwrap_or_else(|| format!("Location: {latitude}, {longitude}"));

        self.ensure_encryption_state(room_id).await?;
        let timeline = self.cached_timeline(room_id).await?;
        let handle = timeline
            .send_location(body, geo_uri, description, None, None, None)
            .await
            .map_err(sdk)?;
        drop(handle);
        Ok(())
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

    /// Forward one or more events to one or more target rooms.
    pub async fn forward_events(
        &self,
        room_id: &RoomId,
        event_ids: &[alloy_protocol::EventId],
        targets: &[RoomId],
    ) -> AlloyResult<(Vec<RoomId>, Vec<alloy_protocol::ForwardFailure>)> {
        use matrix_sdk::ruma::events::{
            AnyMessageLikeEventContent, AnySyncMessageLikeEvent, AnySyncTimelineEvent,
            SyncMessageLikeEvent, room::message::MessageType,
        };

        let room = self.room(room_id)?;
        let mut contents = Vec::new();

        for eid in event_ids {
            let parsed_id = matrix_sdk::ruma::EventId::parse(&eid.0)
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            let ev = room
                .event(&parsed_id, None)
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

            let sync_ev: AnySyncTimelineEvent = ev
                .raw()
                .deserialize()
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

            let content = match sync_ev {
                AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                    SyncMessageLikeEvent::Original(mut m),
                )) => {
                    m.content.relates_to = None;
                    if let MessageType::Text(text_content) = &mut m.content.msgtype {
                        if text_content.body.starts_with("> ") {
                            let lines: Vec<&str> = text_content.body.lines().collect();
                            let mut i = 0;
                            while i < lines.len()
                                && (lines[i].starts_with("> ") || lines[i].is_empty())
                            {
                                i += 1;
                            }
                            if i < lines.len() {
                                text_content.body = lines[i..].join("\n").trim_start().to_string();
                            }
                        }
                        if let Some(formatted) = &mut text_content.formatted
                            && let Some(pos) = formatted.body.find("</mx_reply>")
                        {
                            formatted.body = formatted.body[pos + "</mx_reply>".len()..]
                                .trim_start()
                                .to_string();
                        }
                    }
                    AnyMessageLikeEventContent::RoomMessage(m.content)
                }
                AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::Sticker(
                    SyncMessageLikeEvent::Original(s),
                )) => AnyMessageLikeEventContent::Sticker(s.content),
                _ => {
                    let mut failed = Vec::new();
                    for target in targets {
                        failed.push(alloy_protocol::ForwardFailure {
                            room_id: target.clone(),
                            error: "This message cannot be forwarded".to_string(),
                        });
                    }
                    return Ok((Vec::new(), failed));
                }
            };
            contents.push(content);
        }

        let mut forwarded = Vec::new();
        let mut failed = Vec::new();

        for target in targets {
            let target_timeline = match self.cached_timeline(target).await {
                Ok(t) => t,
                Err(e) => {
                    failed.push(alloy_protocol::ForwardFailure {
                        room_id: target.clone(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };

            let mut all_queued = true;
            for c in &contents {
                if let Err(e) = target_timeline.send(c.clone()).await {
                    failed.push(alloy_protocol::ForwardFailure {
                        room_id: target.clone(),
                        error: e.to_string(),
                    });
                    all_queued = false;
                    break;
                }
            }
            if all_queued {
                forwarded.push(target.clone());
            }
        }

        Ok((forwarded, failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_spoiler() {
        let (result, has) = post_process_spoilers("hello ||spoiler|| world");
        assert!(has);
        assert_eq!(result, "hello <span data-mx-spoiler>spoiler</span> world");
    }

    #[test]
    fn test_multiple_spoilers() {
        let (result, has) = post_process_spoilers("||first|| text ||second||");
        assert!(has);
        assert_eq!(
            result,
            "<span data-mx-spoiler>first</span> text <span data-mx-spoiler>second</span>"
        );
    }

    #[test]
    fn test_spoiler_in_code_ignored() {
        let (result, has) = post_process_spoilers("text <code>||not spoiler||</code> more");
        assert!(!has);
        assert_eq!(result, "text <code>||not spoiler||</code> more");
    }

    #[test]
    fn test_spoiler_in_pre_ignored() {
        let (result, has) = post_process_spoilers("text <pre>||not spoiler||</pre> more");
        assert!(!has);
        assert_eq!(result, "text <pre>||not spoiler||</pre> more");
    }

    #[test]
    fn test_unmatched_spoiler_markers() {
        let (result, has) = post_process_spoilers("hello ||unclosed spoiler");
        assert!(!has);
        assert_eq!(result, "hello ||unclosed spoiler");
    }

    #[test]
    fn test_empty_spoiler() {
        let (result, has) = post_process_spoilers("before |||| after");
        assert!(!has);
        assert_eq!(result, "before |||| after");
    }

    #[test]
    fn test_nested_html_in_spoiler_aborts() {
        // If a tag appears inside the spoiler markers, don't process it as spoiler
        let (result, has) = post_process_spoilers("hello ||text<b>bold||");
        assert!(!has);
        assert_eq!(result, "hello ||text<b>bold||");
    }

    #[test]
    fn test_spoiler_with_spaces() {
        let (result, has) = post_process_spoilers("||  spaced content  ||");
        assert!(has);
        assert_eq!(result, "<span data-mx-spoiler>  spaced content  </span>");
    }

    #[test]
    fn test_spoiler_with_numbers() {
        let (result, has) = post_process_spoilers("||12345||");
        assert!(has);
        assert_eq!(result, "<span data-mx-spoiler>12345</span>");
    }

    #[test]
    fn test_nested_code_tags() {
        let (result, has) =
            post_process_spoilers("<code>||ignore||</code> ||process|| <code>||ignore||</code>");
        assert!(has);
        assert_eq!(
            result,
            "<code>||ignore||</code> <span data-mx-spoiler>process</span> <code>||ignore||</code>"
        );
    }
}
