use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::AlloyError;
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, SubscriptionId};

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
        AlloyError::MatrixSdk(m) => EngineError::network_error(m),
        other => EngineError::internal_error(other.to_string()),
    }
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
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Rooms group",
                name
            ))),
        }
    }
}
