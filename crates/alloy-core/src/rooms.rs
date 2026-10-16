//! Owned by stream core-rooms-timeline

use crate::mapping;
use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{RoomId, RoomSummary, VectorDiffDto};
use futures::StreamExt;
use matrix_sdk::ruma::RoomId as RumaRoomId;
use matrix_sdk::ruma::events::InitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk_ui::room_list_service::RoomListService;

impl AlloyClient {
    /// Create a new room, optionally with encryption.
    pub async fn create_room(&self, name: &str, encrypted: bool) -> AlloyResult<RoomId> {
        use matrix_sdk::ruma::api::client::room::create_room::v3::Request;

        let mut request = Request::new();
        request.name = Some(name.to_string());

        if encrypted {
            request.initial_state = vec![
                InitialStateEvent::with_empty_state_key(
                    RoomEncryptionEventContent::with_recommended_defaults(),
                )
                .to_raw_any(),
            ];
        }

        let response = self
            .client
            .create_room(request)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(RoomId(response.room_id().to_string()))
    }

    /// Open and initialize RoomListService (sliding sync).
    pub async fn room_list_service(&self) -> AlloyResult<RoomListService> {
        let room_list_svc = RoomListService::new(self.client.clone())
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(room_list_svc)
    }

    /// Subscribe to room list changes as a stream of VectorDiffDto<RoomSummary>.
    ///
    /// Returns the initial list of rooms and a stream of diffs.
    pub fn room_list_stream(
        &self,
    ) -> (
        Vec<RoomSummary>,
        impl futures::Stream<Item = Vec<VectorDiffDto<RoomSummary>>> + '_,
    ) {
        let (initial, raw_stream) = self.client.rooms_stream();
        let initial_summaries: Vec<RoomSummary> =
            initial.iter().map(mapping::room_to_summary).collect();

        let stream = raw_stream.map(|diffs| {
            diffs
                .into_iter()
                .map(mapping::map_room_vector_diff)
                .collect()
        });

        (initial_summaries, stream)
    }

    /// Join a room by ID.
    pub async fn join_room(&self, room_id: &RoomId) -> AlloyResult<()> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        self.client
            .join_room_by_id(&parsed_id)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(())
    }

    /// Invite a user to a room.
    pub async fn invite_user(&self, room_id: &RoomId, user_id: &str) -> AlloyResult<()> {
        let parsed_room_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;
        let parsed_user_id = matrix_sdk::ruma::UserId::parse(user_id)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_room_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;

        room.invite_user_by_id(&parsed_user_id)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(())
    }
}
