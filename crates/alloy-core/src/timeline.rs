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
