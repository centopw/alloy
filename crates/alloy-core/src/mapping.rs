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
        )) => m.content.body().to_owned(),
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
