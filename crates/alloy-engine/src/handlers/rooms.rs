use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::AlloyError;
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, NotificationLevel, RoomId, SubscriptionId,
};
use std::path::PathBuf;

/// Handler for room operations and subscriptions (`CommandGroup::Rooms`).
///
/// Owned by stream: `core-rooms-timeline`
#[derive(Default)]
pub struct RoomsHandler;

impl RoomsHandler {
    pub fn new() -> Self {
        Self
    }
}

pub(crate) fn map_err(e: AlloyError) -> EngineError {
    match e {
        AlloyError::NotLoggedIn => EngineError::auth_failed("not logged in"),
        AlloyError::Auth(m) => EngineError::auth_failed(m),
        AlloyError::RoomNotFound(r) => EngineError::not_found(format!("room {r}")),
        AlloyError::InvalidRoomId(m) => EngineError::invalid_argument(m),
        AlloyError::InvalidRoomTag(m) => EngineError::invalid_argument(m),
        AlloyError::InvalidPrefs(m) => EngineError::invalid_argument(m),
        AlloyError::InvalidSearch(m) => EngineError::invalid_argument(m),
        AlloyError::MatrixSdk(m) => EngineError::network_error(m),
        other => EngineError::internal_error(other.to_string()),
    }
}

/// Longest room note the handler accepts, in characters.
const MAX_NOTE_CHARS: usize = 4000;

/// Error for a room setting the active account lacks the power level to change.
fn not_allowed(room_id: &RoomId, setting: &str) -> EngineError {
    EngineError::forbidden(format!(
        "you cannot change the {setting} of room {}",
        room_id.0
    ))
}

#[async_trait::async_trait]
impl GroupHandler for RoomsHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Rooms
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::Unsubscribe { subscription_id } => {
                ctx.subscriptions.cancel(&subscription_id).await;
                Ok(EngineResponse::Unsubscribed { subscription_id })
            }
            EngineCommand::SubscribeRoomList { filter } => {
                let client = ctx.active_client().await?;
                let subscription_id = SubscriptionId(uuid::Uuid::new_v4().to_string());
                let sink = ctx.sink.clone();
                let sid = subscription_id.clone();
                let task = tokio::spawn(async move {
                    let res = client
                        .run_room_list(filter.as_deref(), 50, |diffs| {
                            for diff in diffs {
                                sink.emit(EngineEvent::RoomListDiff {
                                    subscription_id: sid.clone(),
                                    diff,
                                });
                            }
                        })
                        .await;
                    if let Err(e) = res {
                        sink.emit(EngineEvent::ToastNotice {
                            message: format!("Room list stopped: {e}"),
                            level: "error".into(),
                        });
                    }
                });
                ctx.subscriptions
                    .register(subscription_id.clone(), task.abort_handle())
                    .await;
                Ok(EngineResponse::RoomListSubscribed { subscription_id })
            }
            EngineCommand::JoinRoom { room_id_or_alias } => {
                let client = ctx.active_client().await?;
                let room_id = client
                    .join_room_by_id_or_alias(&room_id_or_alias)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomJoined { room_id })
            }
            EngineCommand::LeaveRoom { room_id } => {
                let client = ctx.active_client().await?;
                client.leave_room(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::RoomLeft { room_id })
            }
            EngineCommand::CreateRoom {
                name,
                encrypted,
                invite,
            } => {
                let client = ctx.active_client().await?;
                let room_id = client
                    .create_room(&name, encrypted)
                    .await
                    .map_err(map_err)?;
                for user in invite.unwrap_or_default() {
                    client
                        .invite_user(&room_id, &user.0)
                        .await
                        .map_err(map_err)?;
                }
                Ok(EngineResponse::RoomCreated { room_id })
            }
            EngineCommand::GetRoomMembers { room_id } => {
                let client = ctx.active_client().await?;
                let members = client.room_members(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::RoomMembers { room_id, members })
            }
            EngineCommand::GetRoomDetails { room_id } => {
                let client = ctx.active_client().await?;
                let details = client.room_details(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::RoomDetailsRetrieved { details })
            }
            EngineCommand::SearchPublicRooms {
                account_id,
                query,
                server,
                since,
                limit,
            } => {
                let client = ctx.active_client().await?;
                if client.user_id() != Some(account_id.0.as_str()) {
                    return Err(EngineError::not_found(format!(
                        "account {} is not active",
                        account_id.0
                    )));
                }
                let page = client
                    .search_public_rooms(
                        query.as_deref(),
                        server.as_deref(),
                        since.as_deref(),
                        limit,
                    )
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::PublicRooms {
                    rooms: page.rooms,
                    next_batch: page.next_batch,
                    total_estimate: page.total_estimate,
                })
            }
            EngineCommand::SearchRoomMessages {
                room_id,
                query,
                sender,
                limit,
                next_batch,
            } => {
                let client = ctx.active_client().await?;
                let (hits, next_batch, scope) = client
                    .search_room_messages(
                        &room_id,
                        &query,
                        sender.as_ref(),
                        limit,
                        next_batch.as_deref(),
                    )
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomSearchResults {
                    hits,
                    next_batch,
                    scope,
                })
            }
            EngineCommand::SetRoomName { room_id, name } => {
                let client = ctx.active_client().await?;
                if !client
                    .room_admin_rights(&room_id)
                    .await
                    .map_err(map_err)?
                    .can_set_name
                {
                    return Err(not_allowed(&room_id, "name"));
                }
                client
                    .set_room_name(&room_id, &name)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetRoomTopic { room_id, topic } => {
                let client = ctx.active_client().await?;
                if !client
                    .room_admin_rights(&room_id)
                    .await
                    .map_err(map_err)?
                    .can_set_topic
                {
                    return Err(not_allowed(&room_id, "topic"));
                }
                client
                    .set_room_topic(&room_id, &topic)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetRoomAvatar { room_id, file_path } => {
                let client = ctx.active_client().await?;
                if !client
                    .room_admin_rights(&room_id)
                    .await
                    .map_err(map_err)?
                    .can_set_avatar
                {
                    return Err(not_allowed(&room_id, "avatar"));
                }
                let path = match file_path {
                    Some(file_path) => {
                        let path = PathBuf::from(&file_path);
                        if !ctx.attachment_path_allowed(&path) {
                            return Err(EngineError::forbidden(format!(
                                "attachment {file_path} is not registered for sending"
                            )));
                        }
                        if !path.is_file() {
                            return Err(EngineError::not_found(format!("file {file_path}")));
                        }
                        Some(path)
                    }
                    None => None,
                };
                client
                    .set_room_avatar(&room_id, path.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::GetRoomNote { room_id } => {
                let client = ctx.active_client().await?;
                let note = client.get_room_note(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::RoomNote { room_id, note })
            }
            EngineCommand::SetRoomNote { room_id, note } => {
                if note.chars().count() > MAX_NOTE_CHARS {
                    return Err(EngineError::invalid_argument(format!(
                        "room note exceeds {MAX_NOTE_CHARS} characters"
                    )));
                }
                let client = ctx.active_client().await?;
                client
                    .set_room_note(&room_id, &note)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::GetTagCatalog => {
                let client = ctx.active_client().await?;
                let tags = client.get_tag_catalog().await.map_err(map_err)?;
                Ok(EngineResponse::TagCatalog { tags })
            }
            EngineCommand::SaveTagCatalog { tags } => {
                let client = ctx.active_client().await?;
                client.save_tag_catalog(tags).await.map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetRoomTag {
                room_id,
                tag_id,
                enabled,
            } => {
                let client = ctx.active_client().await?;
                client
                    .set_room_tag(&room_id, &tag_id, enabled)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::DeleteRoomTag { tag_id } => {
                let client = ctx.active_client().await?;
                client.delete_room_tag(&tag_id).await.map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::MarkRoomRead { room_id } => {
                let client = ctx.active_client().await?;
                client.mark_room_read(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::GetUserProfile { user_id, room_id } => {
                let client = ctx.active_client().await?;
                let profile = client
                    .get_user_profile(&user_id, room_id.as_ref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::UserProfile { profile })
            }
            EngineCommand::StartDirectMessage { user_id } => {
                let client = ctx.active_client().await?;
                if client.user_id() == Some(user_id.0.as_str()) {
                    return Err(EngineError::invalid_argument(
                        "cannot start a direct message with yourself".to_owned(),
                    ));
                }
                let (room_id, created) = client
                    .start_direct_message(&user_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::DirectMessageRoom { room_id, created })
            }
            EngineCommand::GetNotificationSettings => {
                let client = ctx.active_client().await?;
                let settings = client.get_notification_settings().await.map_err(map_err)?;
                Ok(EngineResponse::NotificationSettings { settings })
            }
            EngineCommand::SetDefaultNotificationLevel { one_to_one, level } => {
                if level == NotificationLevel::Mute {
                    return Err(EngineError::invalid_argument(
                        "a default notification level cannot be Mute; use All or Mentions",
                    ));
                }
                let client = ctx.active_client().await?;
                client
                    .set_default_notification_level(one_to_one, level)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetMentionRules {
                room_mention,
                user_mention,
                invite,
            } => {
                let client = ctx.active_client().await?;
                client
                    .set_mention_rules(room_mention, user_mention, invite)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::GetRoomNotificationLevel { room_id } => {
                let client = ctx.active_client().await?;
                let state = client
                    .get_room_notification_level(&room_id)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomNotificationLevel {
                    room_id,
                    level: state.level,
                    is_default: state.is_default,
                })
            }
            EngineCommand::SetRoomNotificationLevel { room_id, level } => {
                let client = ctx.active_client().await?;
                client
                    .set_room_notification_level(&room_id, level)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetRoomFavourite { room_id, favourite } => {
                let client = ctx.active_client().await?;
                client
                    .set_room_favourite(&room_id, favourite)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomFavouriteSet { room_id, favourite })
            }
            EngineCommand::RefreshSync => {
                let client = ctx.active_client().await?;
                client.refresh_sync().await.map_err(map_err)?;
                Ok(EngineResponse::SyncRefreshed)
            }
            EngineCommand::SetRoomLowPriority {
                room_id,
                low_priority,
            } => {
                let client = ctx.active_client().await?;
                client
                    .set_room_low_priority(&room_id, low_priority)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomLowPrioritySet {
                    room_id,
                    low_priority,
                })
            }
            EngineCommand::SetRoomMarkedUnread { room_id, unread } => {
                let client = ctx.active_client().await?;
                client
                    .set_room_marked_unread(&room_id, unread)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomMarkedUnreadSet { room_id, unread })
            }
            EngineCommand::SaveDraft {
                room_id,
                text,
                reply_to,
                edit_of,
            } => {
                let client = ctx.active_client().await?;
                client
                    .save_draft(&room_id, text, reply_to, edit_of)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::DraftSaved { room_id })
            }
            EngineCommand::GetDraft { room_id } => {
                let client = ctx.active_client().await?;
                let draft = client.get_draft(&room_id).await.map_err(map_err)?;
                Ok(EngineResponse::Draft { room_id, draft })
            }
            EngineCommand::ListDrafts => {
                let client = ctx.active_client().await?;
                let items = client.list_drafts().await.map_err(map_err)?;
                Ok(EngineResponse::Drafts { items })
            }
            EngineCommand::KickUser {
                room_id,
                user_id,
                reason,
            } => {
                let client = ctx.active_client().await?;
                client
                    .kick_user(&room_id, &user_id, reason.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::UserKicked { room_id, user_id })
            }
            EngineCommand::BanUser {
                room_id,
                user_id,
                reason,
            } => {
                let client = ctx.active_client().await?;
                client
                    .ban_user(&room_id, &user_id, reason.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::UserBanned { room_id, user_id })
            }
            EngineCommand::UnbanUser {
                room_id,
                user_id,
                reason,
            } => {
                let client = ctx.active_client().await?;
                client
                    .unban_user(&room_id, &user_id, reason.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::UserUnbanned { room_id, user_id })
            }
            EngineCommand::SetPowerLevel {
                room_id,
                user_id,
                level,
            } => {
                let client = ctx.active_client().await?;
                client
                    .set_power_level(&room_id, &user_id, level)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::PowerLevelSet {
                    room_id,
                    user_id,
                    level,
                })
            }
            EngineCommand::ReportEvent {
                room_id,
                event_id,
                reason,
            } => {
                let client = ctx.active_client().await?;
                client
                    .report_event(&room_id, &event_id, reason.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::EventReported { room_id, event_id })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Rooms group",
                name
            ))),
        }
    }
}
