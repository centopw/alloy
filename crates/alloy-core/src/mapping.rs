//! Matrix SDK to alloy-protocol DTO mapping.

use alloy_protocol::{NotificationCount, RoomId, RoomMembership, RoomSummary, VectorDiffDto};

/// Convert a matrix-sdk Room to an alloy-protocol RoomSummary DTO.
pub fn room_to_summary(room: &matrix_sdk::Room) -> RoomSummary {
    RoomSummary {
        id: RoomId(room.room_id().to_string()),
        name: room.name(),
        avatar_url: None,
        unread_notifications: NotificationCount {
            highlight_count: 0,
            notification_count: 0,
        },
        latest_event: None,
        is_direct: false,
        is_space: room.is_space(),
        encrypted: false,
        membership: match room.state() {
            matrix_sdk::RoomState::Joined => RoomMembership::Joined,
            matrix_sdk::RoomState::Invited => RoomMembership::Invited,
            matrix_sdk::RoomState::Left => RoomMembership::Left,
            matrix_sdk::RoomState::Banned => RoomMembership::Banned,
            _ => RoomMembership::Left,
        },
        favourite: None,
        low_priority: None,
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
