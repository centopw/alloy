//! Matrix SDK to alloy-protocol DTO mapping.

use alloy_protocol::{
    NotificationCount, RoomId, RoomMembership, RoomSummary, TimelineItemPreview, UserId,
    VectorDiffDto,
};

/// Short text preview of a message-like event, used for room list rows.
fn preview_of(
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
) -> Option<TimelineItemPreview> {
    use matrix_sdk::ruma::events::{
        AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
    };
    let ev = event.raw().deserialize().ok()?;
    let timestamp_millis = u64::from(ev.origin_server_ts().0);
    let sender = UserId(ev.sender().to_string());
    let body = match ev {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
            SyncMessageLikeEvent::Original(m),
        )) => match &m.content.relates_to {
            // An edit: show the new text, not the "* " fallback body.
            Some(matrix_sdk::ruma::events::room::message::Relation::Replacement(r)) => {
                r.new_content.msgtype.body().to_owned()
            }
            _ => m.content.body().to_owned(),
        },
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomEncrypted(_)) => {
            "Encrypted message".to_owned()
        }
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::Sticker(_)) => {
            "Sticker".to_owned()
        }
        _ => return None,
    };
    Some(TimelineItemPreview {
        sender,
        timestamp_millis,
        body,
    })
}

fn latest_preview(room: &matrix_sdk::Room) -> Option<TimelineItemPreview> {
    use matrix_sdk::latest_events::LatestEventValue;
    match room.latest_event() {
        LatestEventValue::Remote(event) => preview_of(&event),
        _ => None,
    }
}

/// Convert a matrix-sdk Room to an alloy-protocol RoomSummary DTO.
pub fn room_to_summary(room: &matrix_sdk::Room) -> RoomSummary {
    let counts = room.unread_notification_counts();
    RoomSummary {
        id: RoomId(room.room_id().to_string()),
        name: room
            .cached_display_name()
            .map(|n| n.to_string())
            .or_else(|| room.name()),
        avatar_url: room.avatar_url().map(|u| u.to_string()),
        unread_notifications: NotificationCount {
            highlight_count: counts.highlight_count,
            notification_count: counts.notification_count,
        },
        latest_event: latest_preview(room),
        is_direct: room.direct_targets_length() > 0,
        is_space: room.is_space(),
        encrypted: room.encryption_state().is_encrypted(),
        membership: match room.state() {
            matrix_sdk::RoomState::Joined => RoomMembership::Joined,
            matrix_sdk::RoomState::Invited => RoomMembership::Invited,
            matrix_sdk::RoomState::Left => RoomMembership::Left,
            matrix_sdk::RoomState::Banned => RoomMembership::Banned,
            _ => RoomMembership::Left,
        },
        favourite: Some(room.is_favourite()),
        low_priority: Some(room.is_low_priority()),
    }
}

/// Map an eyeball-im VectorDiff of matrix-sdk Rooms to a VectorDiffDto of RoomSummary.
pub fn map_room_vector_diff(
    diff: matrix_sdk_ui::eyeball_im::VectorDiff<matrix_sdk::Room>,
) -> VectorDiffDto<RoomSummary> {
    match diff {
        matrix_sdk_ui::eyeball_im::VectorDiff::Append { values } => VectorDiffDto::Append {
            values: values.iter().map(room_to_summary).collect(),
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Clear => VectorDiffDto::Clear,
        matrix_sdk_ui::eyeball_im::VectorDiff::PushFront { value } => VectorDiffDto::PushFront {
            value: room_to_summary(&value),
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::PushBack { value } => VectorDiffDto::PushBack {
            value: room_to_summary(&value),
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::PopFront => VectorDiffDto::PopFront,
        matrix_sdk_ui::eyeball_im::VectorDiff::PopBack => VectorDiffDto::PopBack,
        matrix_sdk_ui::eyeball_im::VectorDiff::Insert { index, value } => VectorDiffDto::Insert {
            index,
            value: room_to_summary(&value),
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Set { index, value } => VectorDiffDto::Set {
            index,
            value: room_to_summary(&value),
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Remove { index } => VectorDiffDto::Remove { index },
        matrix_sdk_ui::eyeball_im::VectorDiff::Truncate { length } => {
            VectorDiffDto::Truncate { length }
        }
        matrix_sdk_ui::eyeball_im::VectorDiff::Reset { values } => VectorDiffDto::Reset {
            values: values.iter().map(room_to_summary).collect(),
        },
    }
}

// ---------------------------------------------------------------------------
// Timeline mapping
// ---------------------------------------------------------------------------

use alloy_protocol::{
    EventId, EventSendState, ReactionSummary, ThreadSummary, TimelineItemContent, TimelineItemDto,
};
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::room::message::MessageType;
use matrix_sdk_ui::timeline::{
    EmbeddedEvent, EventSendState as SdkSendState, EventTimelineItem, MsgLikeKind, TimelineDetails,
    TimelineItem, TimelineItemContent as SdkContent, VirtualTimelineItem,
};

/// Generic eyeball-im diff mapper.
pub fn map_diff<T, U>(
    diff: matrix_sdk_ui::eyeball_im::VectorDiff<T>,
    f: impl Fn(&T) -> U,
) -> VectorDiffDto<U>
where
    T: Clone,
{
    use matrix_sdk_ui::eyeball_im::VectorDiff as D;
    match diff {
        D::Append { values } => VectorDiffDto::Append {
            values: values.iter().map(&f).collect(),
        },
        D::Clear => VectorDiffDto::Clear,
        D::PushFront { value } => VectorDiffDto::PushFront { value: f(&value) },
        D::PushBack { value } => VectorDiffDto::PushBack { value: f(&value) },
        D::PopFront => VectorDiffDto::PopFront,
        D::PopBack => VectorDiffDto::PopBack,
        D::Insert { index, value } => VectorDiffDto::Insert {
            index,
            value: f(&value),
        },
        D::Set { index, value } => VectorDiffDto::Set {
            index,
            value: f(&value),
        },
        D::Remove { index } => VectorDiffDto::Remove { index },
        D::Truncate { length } => VectorDiffDto::Truncate { length },
        D::Reset { values } => VectorDiffDto::Reset {
            values: values.iter().map(&f).collect(),
        },
    }
}

fn millis(ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch) -> u64 {
    u64::from(ts.0)
}

fn media_source_string(source: &MediaSource) -> String {
    match source {
        MediaSource::Plain(uri) => uri.to_string(),
        MediaSource::Encrypted(file) => serde_json::to_string(file).unwrap_or_default(),
    }
}

fn map_message_type(msgtype: &MessageType) -> TimelineItemContent {
    let media = |source: &MediaSource,
                 mime: Option<String>,
                 size: Option<u64>,
                 thumb: Option<String>,
                 body: &str,
                 kind: &str,
                 w: Option<u32>,
                 h: Option<u32>,
                 dur: Option<u64>| TimelineItemContent::Media {
        uri: media_source_string(source),
        mime_type: mime.unwrap_or_else(|| "application/octet-stream".into()),
        size_bytes: size.unwrap_or(0),
        thumbnail_uri: thumb,
        body: body.to_owned(),
        media_kind: kind.to_owned(),
        width: w,
        height: h,
        duration_ms: dur,
    };
    let dim = |v: Option<matrix_sdk::ruma::UInt>| v.and_then(|u| u32::try_from(u64::from(u)).ok());
    match msgtype {
        MessageType::Image(c) => {
            let info = c.info.as_deref();
            media(
                &c.source,
                info.and_then(|i| i.mimetype.clone()),
                info.and_then(|i| i.size).map(u64::from),
                info.and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                c.body.as_str(),
                "image",
                dim(info.and_then(|i| i.width)),
                dim(info.and_then(|i| i.height)),
                None,
            )
        }
        MessageType::Video(c) => {
            let info = c.info.as_deref();
            media(
                &c.source,
                info.and_then(|i| i.mimetype.clone()),
                info.and_then(|i| i.size).map(u64::from),
                info.and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                c.body.as_str(),
                "video",
                dim(info.and_then(|i| i.width)),
                dim(info.and_then(|i| i.height)),
                info.and_then(|i| i.duration).map(|d| d.as_millis() as u64),
            )
        }
        MessageType::Audio(c) => {
            let info = c.info.as_deref();
            media(
                &c.source,
                info.and_then(|i| i.mimetype.clone()),
                info.and_then(|i| i.size).map(u64::from),
                None,
                c.body.as_str(),
                "audio",
                None,
                None,
                info.and_then(|i| i.duration).map(|d| d.as_millis() as u64),
            )
        }
        MessageType::File(c) => {
            let info = c.info.as_deref();
            media(
                &c.source,
                info.and_then(|i| i.mimetype.clone()),
                info.and_then(|i| i.size).map(u64::from),
                info.and_then(|i| i.thumbnail_source.as_ref())
                    .map(media_source_string),
                c.filename(),
                "file",
                None,
                None,
                None,
            )
        }
        MessageType::Emote(c) => TimelineItemContent::TextMessage {
            body: format!("* {}", c.body),
            formatted_body: None,
            html_sanitized: false,
        },
        other => {
            let formatted = match other {
                MessageType::Text(c) => c.formatted.as_ref(),
                MessageType::Notice(c) => c.formatted.as_ref(),
                _ => None,
            }
            .filter(|f| f.format == matrix_sdk::ruma::events::room::message::MessageFormat::Html)
            .map(|f| f.body.clone());
            TimelineItemContent::TextMessage {
                body: other.body().to_owned(),
                formatted_body: formatted,
                // The sdk does not sanitize; the UI must sanitize `formatted_body` before rendering.
                html_sanitized: false,
            }
        }
    }
}

fn map_content(content: &SdkContent) -> (TimelineItemContent, bool) {
    match content {
        SdkContent::MsgLike(m) => match &m.kind {
            MsgLikeKind::Message(msg) => (map_message_type(msg.msgtype()), msg.is_edited()),
            MsgLikeKind::Sticker(s) => {
                let c = s.content();
                (
                    TimelineItemContent::Media {
                        uri: match &c.source {
                            matrix_sdk::ruma::events::sticker::StickerMediaSource::Plain(u) => {
                                u.to_string()
                            }
                            _ => String::new(),
                        },
                        mime_type: c
                            .info
                            .mimetype
                            .clone()
                            .unwrap_or_else(|| "image/png".into()),
                        size_bytes: c.info.size.map(u64::from).unwrap_or(0),
                        thumbnail_uri: None,
                        body: c.body.clone(),
                        media_kind: "sticker".into(),
                        width: c.info.width.and_then(|u| u32::try_from(u64::from(u)).ok()),
                        height: c.info.height.and_then(|u| u32::try_from(u64::from(u)).ok()),
                        duration_ms: None,
                    },
                    false,
                )
            }
            MsgLikeKind::Poll(p) => {
                let r = p.results();
                (
                    TimelineItemContent::Poll {
                        question: r.question,
                        answers: r.answers.into_iter().map(|a| a.text).collect(),
                        end_time: r.end_time.map(|t| u64::from(t.0)),
                    },
                    false,
                )
            }
            MsgLikeKind::Redacted => (TimelineItemContent::Redacted, false),
            MsgLikeKind::UnableToDecrypt(_) => (
                TimelineItemContent::UnableToDecrypt {
                    reason: "Waiting for the encryption keys".into(),
                },
                false,
            ),
            MsgLikeKind::Other(_) | MsgLikeKind::LiveLocation(_) => (
                TimelineItemContent::RoomState {
                    state_type: "unsupported".into(),
                    content_json: String::new(),
                },
                false,
            ),
        },
        other => (
            TimelineItemContent::RoomState {
                state_type: state_label(other).into(),
                content_json: String::new(),
            },
            false,
        ),
    }
}

fn state_label(c: &SdkContent) -> &'static str {
    match c {
        SdkContent::MembershipChange(_) => "membership",
        SdkContent::ProfileChange(_) => "profile",
        SdkContent::OtherState(_) => "state",
        _ => "unsupported",
    }
}

fn map_send_state(item: &EventTimelineItem) -> EventSendState {
    match item.send_state() {
        Some(SdkSendState::NotSentYet { .. }) => EventSendState::Sending {
            transaction_id: item
                .transaction_id()
                .map(|t| t.to_string())
                .unwrap_or_default(),
        },
        Some(SdkSendState::SendingFailed {
            error,
            is_recoverable,
        }) => EventSendState::Failed {
            error: error.to_string(),
            can_retry: *is_recoverable,
        },
        Some(SdkSendState::Sent { event_id }) => EventSendState::Sent {
            event_id: EventId(event_id.to_string()),
        },
        None => EventSendState::Sent {
            event_id: EventId(item.event_id().map(|e| e.to_string()).unwrap_or_default()),
        },
    }
}

fn profile_parts(
    details: &TimelineDetails<matrix_sdk_ui::timeline::Profile>,
) -> (Option<String>, Option<String>) {
    match details {
        TimelineDetails::Ready(p) => (
            p.display_name.clone(),
            p.avatar_url.as_ref().map(|u| u.to_string()),
        ),
        _ => (None, None),
    }
}

fn embedded_to_dto(id: &str, e: &EmbeddedEvent) -> TimelineItemDto {
    let (content, edited) = map_content(&e.content);
    let (name, avatar) = profile_parts(&e.sender_profile);
    TimelineItemDto {
        internal_id: id.to_owned(),
        event_id: Some(EventId(id.to_owned())),
        sender: UserId(e.sender.to_string()),
        timestamp_millis: millis(e.timestamp),
        content,
        send_state: EventSendState::Sent {
            event_id: EventId(id.to_owned()),
        },
        is_own: false,
        reply_to: None,
        thread_summary: None,
        read_receipts: None,
        sender_display_name: name,
        sender_avatar_url: avatar,
        edited,
        reactions: vec![],
        transaction_id: None,
    }
}

/// Map one SDK timeline item (event or virtual) to a DTO.
pub fn timeline_item_to_dto(item: &TimelineItem, my_user_id: Option<&str>) -> TimelineItemDto {
    let internal_id = format!("{:?}", item.unique_id());
    if let Some(ev) = item.as_event() {
        return event_to_dto(&internal_id, ev, my_user_id);
    }
    let content = match item.as_virtual() {
        Some(VirtualTimelineItem::DateDivider(ts)) => TimelineItemContent::DayDivider {
            timestamp_millis: millis(*ts),
        },
        _ => TimelineItemContent::Virtual,
    };
    TimelineItemDto {
        internal_id,
        event_id: None,
        sender: UserId(String::new()),
        timestamp_millis: 0,
        content,
        send_state: EventSendState::Sent {
            event_id: EventId(String::new()),
        },
        is_own: false,
        reply_to: None,
        thread_summary: None,
        read_receipts: None,
        sender_display_name: None,
        sender_avatar_url: None,
        edited: false,
        reactions: vec![],
        transaction_id: None,
    }
}

fn event_to_dto(
    internal_id: &str,
    ev: &EventTimelineItem,
    my_user_id: Option<&str>,
) -> TimelineItemDto {
    let (content, edited) = map_content(ev.content());
    let (name, avatar) = profile_parts(ev.sender_profile());
    let (reactions, reply_to, thread_summary) = match ev.content() {
        SdkContent::MsgLike(m) => (
            m.reactions
                .iter()
                .map(|(key, by_sender)| ReactionSummary {
                    key: key.clone(),
                    count: by_sender.len(),
                    senders: by_sender.keys().map(|u| UserId(u.to_string())).collect(),
                    by_me: my_user_id
                        .map(|me| by_sender.keys().any(|u| u.as_str() == me))
                        .unwrap_or(false),
                })
                .collect(),
            m.in_reply_to.as_ref().and_then(|r| match &r.event {
                TimelineDetails::Ready(e) => {
                    Some(Box::new(embedded_to_dto(r.event_id.as_str(), e)))
                }
                _ => None,
            }),
            m.thread_summary.as_ref().map(|t| ThreadSummary {
                root_event_id: EventId(ev.event_id().map(|e| e.to_string()).unwrap_or_default()),
                reply_count: t.num_replies,
                latest_reply_timestamp: 0,
            }),
        ),
        _ => (vec![], None, None),
    };
    TimelineItemDto {
        internal_id: internal_id.to_owned(),
        event_id: ev.event_id().map(|e| EventId(e.to_string())),
        sender: UserId(ev.sender().to_string()),
        timestamp_millis: millis(ev.timestamp()),
        content,
        send_state: map_send_state(ev),
        is_own: ev.is_own(),
        reply_to,
        thread_summary,
        read_receipts: Some(
            ev.read_receipts()
                .keys()
                .map(|u| UserId(u.to_string()))
                .collect(),
        ),
        sender_display_name: name,
        sender_avatar_url: avatar,
        edited,
        reactions,
        transaction_id: ev.transaction_id().map(|t| t.to_string()),
    }
}
