//! Owned by stream core-rooms-timeline

use crate::mapping;
use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{
    PublicRoomDto, RoomDetails, RoomId, RoomMemberDto, RoomMembership, RoomSummary, SpaceParentDto,
    UserId, UserProfileDto, VectorDiffDto,
};
use futures::StreamExt;
use matrix_sdk::RoomMemberships;
use matrix_sdk::ruma::RoomId as RumaRoomId;
use matrix_sdk::ruma::api::client::config::get_room_account_data;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::api::error::ErrorKind;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk::ruma::events::room::power_levels::UserPowerLevel;
use matrix_sdk::ruma::events::{
    AnyRoomAccountDataEventContent, InitialStateEvent, RoomAccountDataEventType, StateEventType,
};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk_ui::room_list_service::RoomListService;
use std::path::Path;

/// Power level reported for users with unlimited power (room creators in v12+ rooms).
/// `Number.MAX_SAFE_INTEGER`, so it survives a round trip through JavaScript.
const INFINITE_POWER_LEVEL: i64 = 9_007_199_254_740_991;

/// Which room-level settings the active account may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomAdminRights {
    pub can_set_name: bool,
    pub can_set_topic: bool,
    pub can_set_avatar: bool,
}

/// Account-data type holding the private room note as `{"note": "..."}`.
const ROOM_NOTE_EVENT_TYPE: &str = "chat.alloy.room_note";

/// Power level as the protocol reports it; unlimited power maps to `INFINITE_POWER_LEVEL`.
fn power_level_value(level: UserPowerLevel) -> i64 {
    match level {
        UserPowerLevel::Int(level) => i64::from(level),
        _ => INFINITE_POWER_LEVEL,
    }
}

async fn room_rights(room: &matrix_sdk::Room) -> RoomAdminRights {
    let levels = room.power_levels_or_default().await;
    let me = room.own_user_id();
    RoomAdminRights {
        can_set_name: levels.user_can_send_state(me, StateEventType::RoomName),
        can_set_topic: levels.user_can_send_state(me, StateEventType::RoomTopic),
        can_set_avatar: levels.user_can_send_state(me, StateEventType::RoomAvatar),
    }
}

impl AlloyClient {
    /// List joined and invited members, loading the full list from the server when
    /// lazy loading left it incomplete. Sorted by power level (desc), then name.
    pub async fn room_members(&self, room_id: &RoomId) -> AlloyResult<Vec<RoomMemberDto>> {
        let room = self.room(room_id)?;
        let members = room
            .members(RoomMemberships::JOIN | RoomMemberships::INVITE)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let mut out: Vec<RoomMemberDto> = members
            .iter()
            .map(|m| RoomMemberDto {
                user_id: UserId(m.user_id().to_string()),
                display_name: m.display_name().map(str::to_owned),
                avatar_url: m.avatar_url().map(|u| u.to_string()),
                power_level: power_level_value(m.power_level()),
                membership: match m.membership() {
                    MembershipState::Invite => RoomMembership::Invited,
                    _ => RoomMembership::Joined,
                },
            })
            .collect();
        out.sort_by_cached_key(|m| {
            let label = m
                .display_name
                .clone()
                .unwrap_or_else(|| m.user_id.0.clone())
                .to_lowercase();
            (std::cmp::Reverse(m.power_level), label)
        });
        Ok(out)
    }

    /// For each joined room contained in a joined space, the ids of those spaces.
    pub async fn space_parents(&self) -> AlloyResult<Vec<SpaceParentDto>> {
        let service = self
            .spaces
            .get_or_init(|| async {
                std::sync::Arc::new(
                    matrix_sdk_ui::spaces::SpaceService::new(self.client.clone()).await,
                )
            })
            .await
            .clone();
        let mut out = Vec::new();
        for room in self.client.joined_rooms() {
            if room.is_space() {
                continue;
            }
            let parents = service.joined_parent_ids_of_child(room.room_id()).await;
            if !parents.is_empty() {
                out.push(SpaceParentDto {
                    room_id: RoomId(room.room_id().to_string()),
                    spaces: parents.iter().map(|p| RoomId(p.to_string())).collect(),
                });
            }
        }
        Ok(out)
    }

    /// Static room facts for the details drawer.
    pub async fn room_details(&self, room_id: &RoomId) -> AlloyResult<RoomDetails> {
        let room = self.room(room_id)?;
        // Count from the member list: the cached room-info counters lag right after creation.
        let members = room
            .members(RoomMemberships::JOIN | RoomMemberships::INVITE)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let invited = members
            .iter()
            .filter(|m| matches!(m.membership(), MembershipState::Invite))
            .count() as u64;
        let rights = room_rights(&room).await;
        Ok(RoomDetails {
            room_id: room_id.clone(),
            name: room
                .cached_display_name()
                .map(|n| n.to_string())
                .or_else(|| room.name()),
            topic: room.topic(),
            canonical_alias: room.canonical_alias().map(|a| a.to_string()),
            avatar_url: room.avatar_url().map(|u| u.to_string()),
            is_direct: room.direct_targets_length() > 0,
            encrypted: room.encryption_state().is_encrypted(),
            joined_members: members.len() as u64 - invited,
            invited_members: invited,
            can_set_name: rights.can_set_name,
            can_set_topic: rights.can_set_topic,
            can_set_avatar: rights.can_set_avatar,
        })
    }

    /// Whether the active account may change the room's name, topic and avatar.
    pub async fn room_admin_rights(&self, room_id: &RoomId) -> AlloyResult<RoomAdminRights> {
        let room = self.room(room_id)?;
        Ok(room_rights(&room).await)
    }

    /// Rename a room. The homeserver rejects the change without the power level.
    pub async fn set_room_name(&self, room_id: &RoomId, name: &str) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        room.set_name(name.to_owned())
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Change a room's topic. The homeserver rejects the change without the power level.
    pub async fn set_room_topic(&self, room_id: &RoomId, topic: &str) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        room.set_room_topic(topic)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Set the room avatar from a local image file, or remove it when `file_path` is `None`.
    pub async fn set_room_avatar(
        &self,
        room_id: &RoomId,
        file_path: Option<&Path>,
    ) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        match file_path {
            None => {
                room.remove_avatar()
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            }
            Some(path) => {
                let mime = crate::media::mime_guess_from_path(path);
                let data = std::fs::read(path)?;
                room.upload_avatar(&mime, data, None)
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// The active account's private note about a room, read from the homeserver.
    /// Returns an empty string when no note is stored.
    pub async fn get_room_note(&self, room_id: &RoomId) -> AlloyResult<String> {
        let room = self.room(room_id)?;
        let user_id = self
            .client
            .user_id()
            .ok_or(AlloyError::NotLoggedIn)?
            .to_owned();
        let request = get_room_account_data::v3::Request::new(
            user_id,
            room.room_id().to_owned(),
            RoomAccountDataEventType::from(ROOM_NOTE_EVENT_TYPE),
        );
        match self.client.send(request).await {
            Ok(response) => Ok(response
                .account_data
                .get_field::<String>("note")
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
                .unwrap_or_default()),
            Err(e) if matches!(e.client_api_error_kind(), Some(ErrorKind::NotFound)) => {
                Ok(String::new())
            }
            Err(e) => Err(AlloyError::MatrixSdk(e.to_string())),
        }
    }

    /// Store the active account's private note about a room. An empty note clears it.
    pub async fn set_room_note(&self, room_id: &RoomId, note: &str) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        let content = if note.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "note": note })
        };
        let content = Raw::new(&content)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
            .cast_unchecked::<AnyRoomAccountDataEventContent>();
        room.set_account_data_raw(
            RoomAccountDataEventType::from(ROOM_NOTE_EVENT_TYPE),
            content,
        )
        .await
        .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Mark a room as read: a read receipt at the latest event, and the unread flag cleared.
    pub async fn mark_room_read(&self, room_id: &RoomId) -> AlloyResult<()> {
        let room = self.room(room_id)?;
        let timeline = self.cached_timeline(room_id).await?;
        timeline
            .mark_as_read(ReceiptType::Read)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        room.set_unread_flag(false)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Profile of a user. With `room_id`, also the user's membership and power level in that room.
    pub async fn get_user_profile(
        &self,
        user_id: &UserId,
        room_id: Option<&RoomId>,
    ) -> AlloyResult<UserProfileDto> {
        let parsed = matrix_sdk::ruma::UserId::parse(&user_id.0)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let mut display_name = None;
        let mut avatar_url = None;
        let mut membership = None;
        let mut power_level = None;
        if let Some(room_id) = room_id {
            let room = self.room(room_id)?;
            if let Some(member) = room
                .get_member(&parsed)
                .await
                .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?
            {
                display_name = member.display_name().map(str::to_owned);
                avatar_url = member.avatar_url().map(|u| u.to_string());
                membership = Some(member.membership().as_str().to_owned());
                power_level = Some(power_level_value(member.power_level()));
            }
        }
        if display_name.is_none() || avatar_url.is_none() {
            // Not in the room, or the room copy lacks a field: fall back to the global profile.
            match self.client.account().fetch_user_profile_of(&parsed).await {
                Ok(response) => {
                    if display_name.is_none() {
                        display_name = response
                            .get("displayname")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
                    }
                    if avatar_url.is_none() {
                        avatar_url = response
                            .get("avatar_url")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
                    }
                }
                Err(e) if matches!(e.client_api_error_kind(), Some(ErrorKind::NotFound)) => {}
                Err(e) => return Err(AlloyError::MatrixSdk(e.to_string())),
            }
        }
        let is_me = self.client.user_id() == Some(&*parsed);
        let dm_room_id = self
            .client
            .get_dm_room(&parsed)
            .map(|room| RoomId(room.room_id().to_string()));
        Ok(UserProfileDto {
            user_id: user_id.clone(),
            display_name,
            avatar_url,
            membership,
            power_level,
            dm_room_id,
            is_me,
        })
    }

    /// Open the direct-message room with a user, creating it when none exists.
    /// Returns the room id and whether it was created by this call.
    pub async fn start_direct_message(&self, user_id: &UserId) -> AlloyResult<(RoomId, bool)> {
        let parsed = matrix_sdk::ruma::UserId::parse(&user_id.0)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        // Held for the whole lookup and create, so concurrent calls for one user cannot open two rooms.
        let mut known = self.direct_rooms.lock().await;
        if let Some(room_id) = known.get(&user_id.0).cloned() {
            let ruma_id =
                RumaRoomId::parse(&room_id).map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
            // A room created moments ago may not be in the local store until sync echoes it,
            // so only a room the store knows as left or invited counts as gone.
            let usable = self
                .client
                .get_room(&ruma_id)
                .is_none_or(|room| matches!(room.state(), matrix_sdk::RoomState::Joined));
            if usable {
                return Ok((RoomId(room_id), false));
            }
            known.remove(&user_id.0);
        }
        if let Some(room) = self.client.get_dm_room(&parsed) {
            let room_id = room.room_id().to_string();
            known.insert(user_id.0.clone(), room_id.clone());
            return Ok((RoomId(room_id), false));
        }
        let room = self
            .client
            .create_dm(&parsed)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        let room_id = room.room_id().to_string();
        known.insert(user_id.0.clone(), room_id.clone());
        // `create_dm` does not record the room in `m.direct`; mark it so later lookups find it after a restart.
        self.client
            .account()
            .mark_as_dm(room.room_id(), std::slice::from_ref(&parsed))
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok((RoomId(room_id), true))
    }

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

/// One page of public room directory results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicRoomsPage {
    pub rooms: Vec<PublicRoomDto>,
    /// Pagination token for the next page, if the directory has more rooms.
    pub next_batch: Option<String>,
    /// Server's estimate of the total number of matching rooms.
    pub total_estimate: Option<u64>,
}

impl AlloyClient {
    /// Searches the public room directory of `server` (the active homeserver when `None`).
    ///
    /// `query` matches room names, topics and aliases; a blank query lists the directory unfiltered.
    /// `since` is the `next_batch` token of a previous page.
    pub async fn search_public_rooms(
        &self,
        query: Option<&str>,
        server: Option<&str>,
        since: Option<&str>,
        limit: u32,
    ) -> AlloyResult<PublicRoomsPage> {
        use matrix_sdk::ruma::api::client::directory::get_public_rooms_filtered::v3::Request;
        use matrix_sdk::ruma::directory::Filter;
        use matrix_sdk::ruma::{OwnedServerName, UInt};

        let mut filter = Filter::new();
        filter.generic_search_term = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_owned);

        let mut request = Request::new();
        request.filter = filter;
        request.limit = Some(UInt::from(limit));
        request.since = since
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        request.server = server
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                OwnedServerName::try_from(s)
                    .map_err(|e| AlloyError::MatrixSdk(format!("invalid server name {s:?}: {e}")))
            })
            .transpose()?;

        let response = self
            .client
            .public_rooms_filtered(request)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        let rooms = response
            .chunk
            .into_iter()
            .map(|chunk| {
                let joined = self
                    .client
                    .get_room(&chunk.room_id)
                    .is_some_and(|room| room.state() == matrix_sdk::RoomState::Joined);
                PublicRoomDto {
                    room_id: RoomId(chunk.room_id.to_string()),
                    alias: chunk.canonical_alias.map(|alias| alias.to_string()),
                    name: chunk.name,
                    topic: chunk.topic,
                    avatar_url: chunk.avatar_url.map(|uri| uri.to_string()),
                    num_joined_members: u64::from(chunk.num_joined_members),
                    world_readable: chunk.world_readable,
                    guest_can_join: chunk.guest_can_join,
                    joined,
                }
            })
            .collect();

        Ok(PublicRoomsPage {
            rooms,
            next_batch: response.next_batch,
            total_estimate: response.total_room_count_estimate.map(u64::from),
        })
    }
}
