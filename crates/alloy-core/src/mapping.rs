//! Matrix SDK to alloy-protocol DTO mapping.

use crate::room_tags::USER_ROOM_TAG_PREFIX;
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

/// Has the user read everything the room list knows about? True when their read receipt sits on
/// the newest event, or the newest event is their own message.
fn caught_up(room: &matrix_sdk::Room) -> bool {
    use matrix_sdk::latest_events::LatestEventValue;
    let receipt = room.read_receipts().latest_active.map(|r| r.event_id);
    match room.latest_event() {
        LatestEventValue::Remote(event) => {
            let own = event
                .raw()
                .get_field::<matrix_sdk::ruma::OwnedUserId>("sender")
                .ok()
                .flatten()
                .is_some_and(|s| s == room.own_user_id());
            own || (receipt.is_some() && event.event_id() == receipt.as_deref())
        }
        _ => receipt.is_some(),
    }
}

/// Unread counts for the room list.
///
/// The SDK computes counts on the client from the events it holds and the user's own read receipt,
/// so they drop to zero the moment a receipt is sent. The counters a homeserver attaches to a
/// sliding-sync response can stay stale until the next message (which kept the unread dot on after
/// the room was read), and Synapse leaves them at zero.
///
/// A client count above zero wins (raised to the server's when that is larger, as the room list
/// only holds a few events). A client count of zero is trusted only when the user has caught up
/// with the newest event; otherwise it may just mean nothing was computed yet (for example right
/// after a restart) and the server's count stands.
fn unread_counts(room: &matrix_sdk::Room) -> NotificationCount {
    let server = room.unread_notification_counts();
    let caught_up = caught_up(room);
    let pick = |client: u64, server: u64| {
        if client == 0 && caught_up {
            0
        } else {
            client.max(server)
        }
    };
    NotificationCount {
        notification_count: pick(room.num_unread_notifications(), server.notification_count),
        highlight_count: pick(room.num_unread_mentions(), server.highlight_count),
    }
}

/// Convert a matrix-sdk Room to an alloy-protocol RoomSummary DTO.
pub fn room_to_summary(room: &matrix_sdk::Room) -> RoomSummary {
    RoomSummary {
        id: RoomId(room.room_id().to_string()),
        name: room
            .cached_display_name()
            .map(|n| n.to_string())
            .or_else(|| room.name()),
        avatar_url: room.avatar_url().map(|u| u.to_string()),
        unread_notifications: unread_counts(room),
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
        tags: Vec::new(),
        direct_user_id: room
            .direct_targets()
            .into_iter()
            .filter_map(|target| matrix_sdk::ruma::OwnedUserId::try_from(target).ok())
            .min()
            .map(|id| UserId(id.to_string())),
        marked_unread: room.is_marked_unread(),
        draft: None,
    }
}

/// Summary of a room including its custom tags, which the SDK reads asynchronously.
pub async fn room_summary(room: &matrix_sdk::Room) -> RoomSummary {
    let mut summary = room_to_summary(room);
    summary.tags = room_tag_ids(room).await;
    summary
}

/// Summaries of several rooms, in order.
pub async fn room_summaries<'a>(
    rooms: impl Iterator<Item = &'a matrix_sdk::Room>,
) -> Vec<RoomSummary> {
    let mut summaries = Vec::new();
    for room in rooms {
        summaries.push(room_summary(room).await);
    }
    summaries
}

/// Ids of a room's custom tags: `u.<id>` room tags with the prefix stripped.
/// A store error is logged and reads as no tags, so one room cannot break the list.
async fn room_tag_ids(room: &matrix_sdk::Room) -> Vec<String> {
    use matrix_sdk::ruma::events::tag::TagName;
    match room.tags().await {
        Ok(Some(tags)) => tags
            .keys()
            .filter_map(|name| match name {
                TagName::User(user_tag) => user_tag
                    .as_ref()
                    .strip_prefix(USER_ROOM_TAG_PREFIX)
                    .map(str::to_owned),
                _ => None,
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!(room = %room.room_id(), error = %e, "failed to read room tags");
            Vec::new()
        }
    }
}

/// Map an eyeball-im VectorDiff of matrix-sdk Rooms to a VectorDiffDto of RoomSummary.
pub async fn map_room_vector_diff(
    diff: matrix_sdk_ui::eyeball_im::VectorDiff<matrix_sdk::Room>,
) -> VectorDiffDto<RoomSummary> {
    match diff {
        matrix_sdk_ui::eyeball_im::VectorDiff::Append { values } => VectorDiffDto::Append {
            values: room_summaries(values.iter()).await,
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Clear => VectorDiffDto::Clear,
        matrix_sdk_ui::eyeball_im::VectorDiff::PushFront { value } => VectorDiffDto::PushFront {
            value: room_summary(&value).await,
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::PushBack { value } => VectorDiffDto::PushBack {
            value: room_summary(&value).await,
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::PopFront => VectorDiffDto::PopFront,
        matrix_sdk_ui::eyeball_im::VectorDiff::PopBack => VectorDiffDto::PopBack,
        matrix_sdk_ui::eyeball_im::VectorDiff::Insert { index, value } => VectorDiffDto::Insert {
            index,
            value: room_summary(&value).await,
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Set { index, value } => VectorDiffDto::Set {
            index,
            value: room_summary(&value).await,
        },
        matrix_sdk_ui::eyeball_im::VectorDiff::Remove { index } => VectorDiffDto::Remove { index },
        matrix_sdk_ui::eyeball_im::VectorDiff::Truncate { length } => {
            VectorDiffDto::Truncate { length }
        }
        matrix_sdk_ui::eyeball_im::VectorDiff::Reset { values } => VectorDiffDto::Reset {
            values: room_summaries(values.iter()).await,
        },
    }
}

// ---------------------------------------------------------------------------
// Timeline mapping
// ---------------------------------------------------------------------------

use alloy_protocol::{
    EventId, EventSendState, PollAnswerDto, ReactionSummary, ThreadSummary, TimelineItemContent,
    TimelineItemDto,
};
use matrix_sdk::ruma::events::StateEventContentChange;
use matrix_sdk::ruma::events::poll::start::PollKind;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::room::message::MessageType;
use matrix_sdk_ui::timeline::{
    AnyOtherStateEventContentChange, EmbeddedEvent, EventSendState as SdkSendState,
    EventTimelineItem, MemberProfileChange, MembershipChange, MsgLikeKind, OtherState, Profile,
    RoomMembershipChange, TimelineDetails, TimelineItem, TimelineItemContent as SdkContent,
    VirtualTimelineItem,
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

pub(crate) fn millis(ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch) -> u64 {
    u64::from(ts.0)
}

pub(crate) fn media_source_string(source: &MediaSource) -> String {
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
        voice: false,
        waveform: None,
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
            let voice = c.voice.is_some();
            let waveform: Option<Vec<u16>> = c.audio.as_ref().map(|a| {
                a.waveform
                    .iter()
                    .map(|w| u64::from(w.get()) as u16)
                    .collect()
            });
            let duration = info
                .and_then(|i| i.duration)
                .or_else(|| c.audio.as_ref().map(|a| a.duration))
                .map(|d| d.as_millis() as u64);
            TimelineItemContent::Media {
                uri: media_source_string(&c.source),
                mime_type: info
                    .and_then(|i| i.mimetype.clone())
                    .unwrap_or_else(|| "audio/ogg".into()),
                size_bytes: info.and_then(|i| i.size).map(u64::from).unwrap_or(0),
                thumbnail_uri: None,
                body: c.body.clone(),
                media_kind: "audio".into(),
                width: None,
                height: None,
                duration_ms: duration,
                voice,
                waveform,
            }
        }
        MessageType::Location(c) => TimelineItemContent::Location {
            geo_uri: c.geo_uri.clone(),
            description: if c.body.is_empty() {
                None
            } else {
                Some(c.body.clone())
            },
            live: false,
            live_until_millis: None,
        },
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

fn map_content(
    content: &SdkContent,
    sender: &matrix_sdk::ruma::UserId,
    sender_profile: &TimelineDetails<Profile>,
    my_user_id: Option<&str>,
) -> (TimelineItemContent, bool) {
    let actor = display_or_localpart(profile_display_name(sender_profile), sender);
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
                        voice: false,
                        waveform: None,
                    },
                    false,
                )
            }
            MsgLikeKind::Poll(p) => {
                let r = p.results();
                let has_ended = r.end_time.is_some();
                // Undisclosed polls hide counts until they end. `my_votes` is the caller's own
                // choice, so it stays visible either way.
                let reveal = matches!(r.kind, PollKind::Disclosed) || has_ended;
                let mut voters = std::collections::HashSet::new();
                for list in r.votes.values() {
                    voters.extend(list.iter().map(String::as_str));
                }
                let my_votes: Vec<String> = r
                    .answers
                    .iter()
                    .filter(|a| {
                        my_user_id.is_some_and(|me| {
                            r.votes
                                .get(&a.id)
                                .is_some_and(|list| list.iter().any(|v| v.as_str() == me))
                        })
                    })
                    .map(|a| a.id.clone())
                    .collect();
                let answers: Vec<PollAnswerDto> = r
                    .answers
                    .iter()
                    .map(|a| PollAnswerDto {
                        id: a.id.clone(),
                        text: a.text.clone(),
                        votes: match r.votes.get(&a.id) {
                            Some(list) if reveal => u32::try_from(list.len()).unwrap_or(u32::MAX),
                            _ => 0,
                        },
                    })
                    .collect();
                let total_votes = if reveal {
                    u32::try_from(voters.len()).unwrap_or(u32::MAX)
                } else {
                    0
                };
                (
                    TimelineItemContent::Poll {
                        question: r.question,
                        answers,
                        disclosed: matches!(r.kind, PollKind::Disclosed),
                        max_selections: u32::try_from(r.max_selections).unwrap_or(u32::MAX),
                        total_votes,
                        has_ended,
                        end_time: r.end_time.map(|t| u64::from(t.0)),
                        my_votes,
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
            MsgLikeKind::LiveLocation(l) => {
                let geo_uri = l
                    .latest_location()
                    .map(|b| b.geo_uri().to_owned())
                    .unwrap_or_default();
                let until = u64::from(l.ts().0).saturating_add(l.timeout().as_millis() as u64);
                (
                    TimelineItemContent::Location {
                        geo_uri,
                        description: l.description().map(str::to_owned),
                        live: l.is_live(),
                        live_until_millis: Some(until),
                    },
                    false,
                )
            }
            MsgLikeKind::Other(_) => (
                TimelineItemContent::RoomState {
                    state_type: "unsupported".into(),
                    content_json: String::new(),
                },
                false,
            ),
        },
        SdkContent::MembershipChange(m) => (
            room_state("membership", membership_summary(m, &actor, sender)),
            false,
        ),
        SdkContent::ProfileChange(p) => (
            room_state("profile", profile_summary(p, &actor, sender)),
            false,
        ),
        SdkContent::OtherState(s) => {
            let (state_type, summary) = other_state_summary(s, &actor);
            (room_state(state_type, summary), false)
        }
        _ => (
            TimelineItemContent::RoomState {
                state_type: "unsupported".into(),
                content_json: String::new(),
            },
            false,
        ),
    }
}

fn room_state(state_type: &str, summary: String) -> TimelineItemContent {
    TimelineItemContent::RoomState {
        state_type: state_type.into(),
        content_json: serde_json::json!({ "summary": summary }).to_string(),
    }
}

fn profile_display_name(details: &TimelineDetails<Profile>) -> Option<&str> {
    match details {
        TimelineDetails::Ready(p) => p.display_name.as_deref(),
        _ => None,
    }
}

fn non_blank(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// Display name when present and non-blank, otherwise the user's localpart.
fn display_or_localpart(name: Option<&str>, user: &matrix_sdk::ruma::UserId) -> String {
    non_blank(name)
        .map(str::to_owned)
        .unwrap_or_else(|| user.localpart().to_owned())
}

/// Collapses all whitespace runs (including newlines) into single spaces.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncates to at most `max` characters, counting the trailing ellipsis.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Names used by the summary sentences. Free of SDK types so the wording is unit-testable.
struct Actors<'a> {
    actor: &'a str,
    target: &'a str,
    same_user: bool,
}

impl Actors<'_> {
    /// "their" when the actor is the target, otherwise "{target}'s".
    fn possessive(&self) -> String {
        if self.same_user {
            "their".to_owned()
        } else {
            format!("{}'s", self.target)
        }
    }

    /// "themselves" when the actor is the target, otherwise the target name.
    fn object(&self) -> &str {
        if self.same_user {
            "themselves"
        } else {
            self.target
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipAction {
    Joined,
    Left,
    Invited,
    InvitationDeclined,
    InvitationRevoked,
    Kicked,
    Banned,
    Unbanned,
    KickedAndBanned,
    Knocked,
    KnockAccepted,
    KnockRetracted,
    KnockDenied,
    Unknown,
}

fn membership_action(change: Option<MembershipChange>) -> MembershipAction {
    match change {
        Some(MembershipChange::Joined | MembershipChange::InvitationAccepted) => {
            MembershipAction::Joined
        }
        Some(MembershipChange::Left) => MembershipAction::Left,
        Some(MembershipChange::Invited) => MembershipAction::Invited,
        Some(MembershipChange::InvitationRejected) => MembershipAction::InvitationDeclined,
        Some(MembershipChange::InvitationRevoked) => MembershipAction::InvitationRevoked,
        Some(MembershipChange::Kicked) => MembershipAction::Kicked,
        Some(MembershipChange::Banned) => MembershipAction::Banned,
        Some(MembershipChange::Unbanned) => MembershipAction::Unbanned,
        Some(MembershipChange::KickedAndBanned) => MembershipAction::KickedAndBanned,
        Some(MembershipChange::Knocked) => MembershipAction::Knocked,
        Some(MembershipChange::KnockAccepted) => MembershipAction::KnockAccepted,
        Some(MembershipChange::KnockRetracted) => MembershipAction::KnockRetracted,
        Some(MembershipChange::KnockDenied) => MembershipAction::KnockDenied,
        Some(
            MembershipChange::None | MembershipChange::Error | MembershipChange::NotImplemented,
        )
        | None => MembershipAction::Unknown,
    }
}

const MAX_REASON_CHARS: usize = 80;

/// Reason text appended to membership sentences, only when it is short and non-blank.
fn short_reason(reason: Option<&str>) -> Option<String> {
    let reason = one_line(reason?);
    (!reason.is_empty() && reason.chars().count() <= MAX_REASON_CHARS).then_some(reason)
}

fn membership_sentence(
    actors: &Actors<'_>,
    action: MembershipAction,
    reason: Option<&str>,
) -> String {
    let who = actors.actor;
    let object = actors.object();
    let sentence = match action {
        MembershipAction::Joined => format!("{who} joined the room"),
        MembershipAction::Left => format!("{who} left the room"),
        MembershipAction::Invited => format!("{who} invited {object}"),
        MembershipAction::InvitationDeclined => format!("{who} declined the invitation"),
        MembershipAction::InvitationRevoked => {
            format!("{who} withdrew the invitation for {object}")
        }
        MembershipAction::Kicked => format!("{who} kicked {object}"),
        MembershipAction::Banned => format!("{who} banned {object}"),
        MembershipAction::Unbanned => format!("{who} unbanned {object}"),
        MembershipAction::KickedAndBanned => format!("{who} kicked and banned {object}"),
        MembershipAction::Knocked => format!("{who} requested to join the room"),
        MembershipAction::KnockAccepted => {
            format!("{who} accepted the request to join from {object}")
        }
        MembershipAction::KnockRetracted => {
            format!("{who} withdrew their request to join the room")
        }
        MembershipAction::KnockDenied => {
            format!("{who} denied the request to join from {object}")
        }
        MembershipAction::Unknown => format!("{who} changed {} profile", actors.possessive()),
    };
    match short_reason(reason) {
        Some(r) => format!("{sentence} (reason: {r})"),
        None => sentence,
    }
}

fn membership_summary(
    m: &RoomMembershipChange,
    actor: &str,
    sender: &matrix_sdk::ruma::UserId,
) -> String {
    let target_id = m.user_id();
    let target = display_or_localpart(m.display_name().as_deref(), target_id);
    let reason = match m.content() {
        StateEventContentChange::Original { content, .. } => content.reason.as_deref(),
        StateEventContentChange::Redacted(_) => None,
    };
    membership_sentence(
        &Actors {
            actor,
            target: &target,
            same_user: target_id == sender,
        },
        membership_action(m.change()),
        reason,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayNameChange<'a> {
    Set(&'a str),
    Removed,
}

/// A display name change from old to new; `None` when neither side has a usable name.
fn display_name_change<'a>(
    old: Option<&'a str>,
    new: Option<&'a str>,
) -> Option<DisplayNameChange<'a>> {
    match (non_blank(new), non_blank(old)) {
        (Some(new), _) => Some(DisplayNameChange::Set(new)),
        (None, Some(_)) => Some(DisplayNameChange::Removed),
        (None, None) => None,
    }
}

fn profile_sentence(
    actors: &Actors<'_>,
    name: Option<DisplayNameChange<'_>>,
    avatar_changed: bool,
) -> String {
    let who = actors.actor;
    let poss = actors.possessive();
    match (name, avatar_changed) {
        (Some(DisplayNameChange::Set(n)), true) => format!(
            "{who} changed {poss} display name to {} and {poss} profile picture",
            one_line(n)
        ),
        (Some(DisplayNameChange::Set(n)), false) => {
            format!("{who} changed {poss} display name to {}", one_line(n))
        }
        (Some(DisplayNameChange::Removed), true) => {
            format!("{who} removed {poss} display name and changed {poss} profile picture")
        }
        (Some(DisplayNameChange::Removed), false) => {
            format!("{who} removed {poss} display name")
        }
        (None, true) => format!("{who} changed {poss} profile picture"),
        (None, false) => format!("{who} changed {poss} profile"),
    }
}

fn profile_summary(
    p: &MemberProfileChange,
    actor: &str,
    sender: &matrix_sdk::ruma::UserId,
) -> String {
    let target_id = p.user_id();
    let name_change = p.displayname_change();
    let target = display_or_localpart(
        name_change.and_then(|c| non_blank(c.new.as_deref()).or(non_blank(c.old.as_deref()))),
        target_id,
    );
    let name = name_change.and_then(|c| display_name_change(c.old.as_deref(), c.new.as_deref()));
    profile_sentence(
        &Actors {
            actor,
            target: &target,
            same_user: target_id == sender,
        },
        name,
        p.avatar_url_change().is_some(),
    )
}

fn room_name_sentence(actor: &str, name: &str) -> String {
    let name = one_line(name);
    if name.is_empty() {
        format!("{actor} removed the room name")
    } else {
        format!("{actor} renamed the room to {name}")
    }
}

const MAX_TOPIC_CHARS: usize = 120;

fn topic_sentence(actor: &str, topic: &str) -> String {
    let topic = one_line(topic);
    if topic.is_empty() {
        format!("{actor} removed the topic")
    } else {
        format!(
            "{actor} changed the topic to {}",
            truncate_chars(&topic, MAX_TOPIC_CHARS)
        )
    }
}

fn avatar_sentence(actor: &str, removed: bool) -> String {
    if removed {
        format!("{actor} removed the room picture")
    } else {
        format!("{actor} changed the room picture")
    }
}

fn encryption_sentence(actor: &str) -> String {
    format!("{actor} turned on end-to-end encryption")
}

fn other_state_sentence(actor: &str) -> String {
    format!("{actor} updated the room settings")
}

/// Room-level state summary. Redacted or unrecognised state falls back to a generic sentence.
fn other_state_summary(s: &OtherState, actor: &str) -> (&'static str, String) {
    match s.content() {
        AnyOtherStateEventContentChange::RoomName(StateEventContentChange::Original {
            content,
            ..
        }) => ("name", room_name_sentence(actor, &content.name)),
        AnyOtherStateEventContentChange::RoomTopic(StateEventContentChange::Original {
            content,
            ..
        }) => ("topic", topic_sentence(actor, &content.topic)),
        AnyOtherStateEventContentChange::RoomAvatar(StateEventContentChange::Original {
            content,
            ..
        }) => ("avatar", avatar_sentence(actor, content.url.is_none())),
        AnyOtherStateEventContentChange::RoomEncryption(StateEventContentChange::Original {
            ..
        }) => ("encryption", encryption_sentence(actor)),
        _ => ("state", other_state_sentence(actor)),
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

fn embedded_to_dto(id: &str, e: &EmbeddedEvent, my_user_id: Option<&str>) -> TimelineItemDto {
    let (content, edited) = map_content(&e.content, &e.sender, &e.sender_profile, my_user_id);
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
        mentions_me: false,
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
        mentions_me: false,
    }
}

fn event_to_dto(
    internal_id: &str,
    ev: &EventTimelineItem,
    my_user_id: Option<&str>,
) -> TimelineItemDto {
    let (content, edited) = map_content(ev.content(), ev.sender(), ev.sender_profile(), my_user_id);
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
                TimelineDetails::Ready(e) => Some(Box::new(embedded_to_dto(
                    r.event_id.as_str(),
                    e,
                    my_user_id,
                ))),
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
        mentions_me: ev.is_highlighted(),
    }
}

/// Minimal TimelineItemDto for a pinned event loaded through `room.event(&id, None)`.
pub fn pinned_event_to_dto(
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
    event_id: &EventId,
    sender_name: Option<String>,
    sender_avatar: Option<String>,
) -> Option<TimelineItemDto> {
    use matrix_sdk::ruma::events::{
        AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
    };
    let ev = event.raw().deserialize().ok()?;
    let timestamp_millis = u64::from(ev.origin_server_ts().0);
    let sender = UserId(ev.sender().to_string());
    let (body, formatted_body) = match &ev {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
            SyncMessageLikeEvent::Original(m),
        )) => match &m.content.relates_to {
            Some(matrix_sdk::ruma::events::room::message::Relation::Replacement(r)) => {
                let fmt = match &r.new_content.msgtype {
                    MessageType::Text(t) => t.formatted.as_ref().map(|f| f.body.clone()),
                    _ => None,
                };
                (r.new_content.msgtype.body().to_owned(), fmt)
            }
            _ => {
                let fmt = match &m.content.msgtype {
                    MessageType::Text(t) => t.formatted.as_ref().map(|f| f.body.clone()),
                    _ => None,
                };
                (m.content.body().to_owned(), fmt)
            }
        },
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomEncrypted(_)) => {
            ("Encrypted message".to_owned(), None)
        }
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::Sticker(_)) => {
            ("Sticker".to_owned(), None)
        }
        _ => return None,
    };
    Some(TimelineItemDto {
        internal_id: event_id.0.clone(),
        event_id: Some(event_id.clone()),
        sender,
        timestamp_millis,
        content: TimelineItemContent::TextMessage {
            body,
            formatted_body,
            html_sanitized: true,
        },
        send_state: EventSendState::Sent {
            event_id: event_id.clone(),
        },
        is_own: false,
        reply_to: None,
        thread_summary: None,
        read_receipts: None,
        sender_display_name: sender_name,
        sender_avatar_url: sender_avatar,
        edited: false,
        reactions: vec![],
        transaction_id: None,
        mentions_me: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice_self() -> Actors<'static> {
        Actors {
            actor: "Alice",
            target: "Alice",
            same_user: true,
        }
    }

    fn alice_on_bob() -> Actors<'static> {
        Actors {
            actor: "Alice",
            target: "Bob",
            same_user: false,
        }
    }

    fn parts(content: TimelineItemContent) -> (String, String) {
        match content {
            TimelineItemContent::RoomState {
                state_type,
                content_json,
            } => (state_type, content_json),
            _ => panic!("expected RoomState"),
        }
    }

    #[test]
    fn display_name_falls_back_to_localpart() {
        let user = matrix_sdk::ruma::UserId::parse("@alice:example.org").unwrap();
        assert_eq!(display_or_localpart(None, &user), "alice");
        assert_eq!(display_or_localpart(Some("   "), &user), "alice");
        assert_eq!(display_or_localpart(Some(" Alice A "), &user), "Alice A");
    }

    #[test]
    fn membership_sentences_put_the_actor_first() {
        let s = membership_sentence;
        assert_eq!(
            s(&alice_self(), MembershipAction::Joined, None),
            "Alice joined the room"
        );
        assert_eq!(
            s(&alice_self(), MembershipAction::Left, None),
            "Alice left the room"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::Invited, None),
            "Alice invited Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::InvitationDeclined, None),
            "Alice declined the invitation"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::InvitationRevoked, None),
            "Alice withdrew the invitation for Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::Unbanned, None),
            "Alice unbanned Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::KickedAndBanned, None),
            "Alice kicked and banned Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::KnockAccepted, None),
            "Alice accepted the request to join from Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::KnockDenied, None),
            "Alice denied the request to join from Bob"
        );
        assert_eq!(
            s(&alice_self(), MembershipAction::Knocked, None),
            "Alice requested to join the room"
        );
        assert_eq!(
            s(&alice_self(), MembershipAction::KnockRetracted, None),
            "Alice withdrew their request to join the room"
        );
    }

    #[test]
    fn membership_reason_is_appended_only_when_short() {
        let s = membership_sentence;
        assert_eq!(
            s(
                &alice_on_bob(),
                MembershipAction::Kicked,
                Some("  spam\n here ")
            ),
            "Alice kicked Bob (reason: spam here)"
        );
        let max = "x".repeat(MAX_REASON_CHARS);
        assert!(s(&alice_on_bob(), MembershipAction::Kicked, Some(&max)).ends_with("x)"));
        let too_long = "x".repeat(MAX_REASON_CHARS + 1);
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::Kicked, Some(&too_long)),
            "Alice kicked Bob"
        );
        assert_eq!(
            s(&alice_on_bob(), MembershipAction::Banned, Some("   ")),
            "Alice banned Bob"
        );
    }

    #[test]
    fn membership_fallback_describes_a_profile_change() {
        assert_eq!(
            membership_sentence(&alice_self(), MembershipAction::Unknown, None),
            "Alice changed their profile"
        );
        assert_eq!(
            membership_sentence(&alice_on_bob(), MembershipAction::Unknown, None),
            "Alice changed Bob's profile"
        );
    }

    #[test]
    fn membership_action_maps_sdk_changes() {
        assert_eq!(
            membership_action(Some(MembershipChange::InvitationAccepted)),
            MembershipAction::Joined
        );
        assert_eq!(
            membership_action(Some(MembershipChange::InvitationRejected)),
            MembershipAction::InvitationDeclined
        );
        assert_eq!(
            membership_action(Some(MembershipChange::KickedAndBanned)),
            MembershipAction::KickedAndBanned
        );
        assert_eq!(
            membership_action(Some(MembershipChange::NotImplemented)),
            MembershipAction::Unknown
        );
        assert_eq!(membership_action(None), MembershipAction::Unknown);
    }

    #[test]
    fn display_name_change_distinguishes_set_removed_and_unchanged() {
        assert_eq!(
            display_name_change(Some("Old"), Some("New")),
            Some(DisplayNameChange::Set("New"))
        );
        assert_eq!(
            display_name_change(Some("Old"), None),
            Some(DisplayNameChange::Removed)
        );
        assert_eq!(
            display_name_change(Some("Old"), Some("  ")),
            Some(DisplayNameChange::Removed)
        );
        assert_eq!(display_name_change(None, None), None);
    }

    #[test]
    fn profile_sentences_cover_name_and_avatar_changes() {
        let s = profile_sentence;
        assert_eq!(
            s(&alice_self(), Some(DisplayNameChange::Set("Ali")), false),
            "Alice changed their display name to Ali"
        );
        assert_eq!(
            s(
                &alice_on_bob(),
                Some(DisplayNameChange::Set("Bobby")),
                false
            ),
            "Alice changed Bob's display name to Bobby"
        );
        assert_eq!(
            s(&alice_self(), Some(DisplayNameChange::Removed), false),
            "Alice removed their display name"
        );
        assert_eq!(
            s(&alice_self(), None, true),
            "Alice changed their profile picture"
        );
        assert_eq!(
            s(&alice_self(), Some(DisplayNameChange::Set("Ali")), true),
            "Alice changed their display name to Ali and their profile picture"
        );
        assert_eq!(
            s(&alice_self(), Some(DisplayNameChange::Removed), true),
            "Alice removed their display name and changed their profile picture"
        );
        assert_eq!(s(&alice_self(), None, false), "Alice changed their profile");
        assert_eq!(
            s(&alice_on_bob(), None, false),
            "Alice changed Bob's profile"
        );
    }

    #[test]
    fn room_name_and_topic_sentences() {
        assert_eq!(
            room_name_sentence("Alice", "Lounge"),
            "Alice renamed the room to Lounge"
        );
        assert_eq!(
            room_name_sentence("Alice", "  "),
            "Alice removed the room name"
        );
        assert_eq!(
            topic_sentence("Alice", "Weekly\n\n  sync"),
            "Alice changed the topic to Weekly sync"
        );
        assert_eq!(topic_sentence("Alice", ""), "Alice removed the topic");
    }

    #[test]
    fn long_topics_are_truncated_with_an_ellipsis() {
        let topic = "word ".repeat(60);
        let sentence = topic_sentence("Alice", &topic);
        let shown = sentence
            .strip_prefix("Alice changed the topic to ")
            .unwrap();
        assert_eq!(shown.chars().count(), MAX_TOPIC_CHARS);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn truncate_counts_the_ellipsis() {
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_chars("abcd", 3), "ab…");
    }

    #[test]
    fn avatar_encryption_and_other_state_sentences() {
        assert_eq!(
            avatar_sentence("Alice", false),
            "Alice changed the room picture"
        );
        assert_eq!(
            avatar_sentence("Alice", true),
            "Alice removed the room picture"
        );
        assert_eq!(
            encryption_sentence("Alice"),
            "Alice turned on end-to-end encryption"
        );
        assert_eq!(
            other_state_sentence("Alice"),
            "Alice updated the room settings"
        );
    }

    #[test]
    fn room_state_carries_the_summary_as_json() {
        let (state_type, json) = parts(room_state(
            "name",
            "Alice renamed the room to Lounge".into(),
        ));
        assert_eq!(state_type, "name");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "summary": "Alice renamed the room to Lounge" })
        );
    }
}
