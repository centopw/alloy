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

        let room_id = RoomId(response.room_id().to_string());
        if encrypted {
            self.await_encryption_ready(&room_id).await?;
        }
        Ok(room_id)
    }

    /// Block until a room we just created as encrypted is known to be encrypted locally.
    ///
    /// A running sync can briefly report the new room as unencrypted before the
    /// `m.room.encryption` state arrives; sending in that window would go out in plaintext.
    async fn await_encryption_ready(&self, room_id: &RoomId) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        for _ in 0..150 {
            if room
                .latest_encryption_state()
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
                .is_encrypted()
            {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Err(AlloyError::MatrixSdk(
            "the new room did not become encrypted; refusing to use it".into(),
        ))
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

    /// Join a room by ID or alias (`#room:server`). Returns the joined room id.
    pub async fn join_room_by_id_or_alias(&self, id_or_alias: &str) -> AlloyResult<RoomId> {
        let parsed = matrix_sdk::ruma::RoomOrAliasId::parse(id_or_alias)
            .map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;
        let room = self
            .client
            .join_room_by_id_or_alias(&parsed, &[])
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(RoomId(room.room_id().to_string()))
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

impl AlloyClient {
    /// Start sync (if needed) and drive the recency-sorted room list, calling `on_diffs`
    /// for every batch of DTO diffs (the first batch is the initial state).
    ///
    /// Runs until the returned future is dropped/aborted or the stream ends, so callers
    /// spawn it as a task and cancel it to unsubscribe. `name_filter` fuzzy-matches room names.
    pub async fn run_room_list(
        &self,
        name_filter: Option<&str>,
        page_size: usize,
        mut on_diffs: impl FnMut(Vec<VectorDiffDto<RoomSummary>>) + Send,
    ) -> AlloyResult<()> {
        use matrix_sdk_ui::room_list_service::filters;
        let sync = self.sync_service().await?;
        let rls = sync.room_list_service();
        let list = rls
            .all_rooms()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let (stream, controller) = list.entries_with_dynamic_adapters(page_size);
        match name_filter.map(str::trim).filter(|f| !f.is_empty()) {
            Some(f) => {
                controller.set_filter(Box::new(filters::new_filter_fuzzy_match_room_name(f)))
            }
            None => controller.set_filter(Box::new(filters::new_filter_non_left())),
        };
        futures::pin_mut!(stream);
        while let Some(diffs) = stream.next().await {
            on_diffs(
                diffs
                    .into_iter()
                    .map(|d| d.map(|item| item.into_inner()))
                    .map(mapping::map_room_vector_diff)
                    .collect(),
            );
        }
        Ok(())
    }

    /// Leave a room.
    pub async fn leave_room(&self, room_id: &RoomId) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        room.leave()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))
    }

    /// Send a typing notice.
    pub async fn set_typing(&self, room_id: &RoomId, typing: bool) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        room.typing_notice(typing)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))
    }

    /// Look up a known room.
    pub(crate) fn room(&self, room_id: &RoomId) -> AlloyResult<matrix_sdk::Room> {
        let parsed =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;
        self.client
            .get_room(&parsed)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))
    }
}
