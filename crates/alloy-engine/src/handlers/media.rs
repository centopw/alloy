use super::rooms::map_err;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse, MediaSource};
use std::path::PathBuf;
use std::time::Duration;

/// Minimum spacing between `UploadProgress` events for one upload.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Handler for media operations (`CommandGroup::Media`).
///
/// Owned by stream: `core-media`. Downloads go through the SDK (authenticated media, E2EE
/// attachments decrypted) into `<data_dir>/media_cache`; the UI loads the returned local path.
#[derive(Default)]
pub struct MediaHandler;

impl MediaHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl GroupHandler for MediaHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Media
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::GetMedia {
                source,
                kind,
                width,
                height,
                mime_type,
            } => match source {
                MediaSource::Remote { uri } => {
                    let thumbnail = match (kind.as_str(), width, height) {
                        ("thumbnail", Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
                        ("thumbnail", _, _) => {
                            return Err(EngineError::invalid_argument(
                                "thumbnail requires non-zero width and height",
                            ));
                        }
                        ("file", _, _) => None,
                        (other, _, _) => {
                            return Err(EngineError::invalid_argument(format!(
                                "unknown media kind '{other}'"
                            )));
                        }
                    };
                    let client = ctx.active_client().await?;
                    let path = client
                        .fetch_media(
                            &uri,
                            mime_type.as_deref(),
                            thumbnail,
                            &ctx.data_dir.join("media_cache"),
                        )
                        .await
                        .map_err(map_err)?;
                    Ok(EngineResponse::MediaRetrieved {
                        local_path: path.to_string_lossy().into_owned(),
                    })
                }
            },
            EngineCommand::SendAttachment {
                room_id,
                file_path,
                mime_type,
                txn_id,
            } => {
                let path = PathBuf::from(&file_path);
                if !ctx.attachment_path_allowed(&path) {
                    return Err(EngineError::forbidden(format!(
                        "attachment {file_path} is not registered for sending"
                    )));
                }
                if !path.is_file() {
                    return Err(EngineError::not_found(format!("file {file_path}")));
                }
                let client = ctx.active_client().await?;
                let txn_id = txn_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let sink = ctx.sink.clone();
                let (event_room, event_txn) = (room_id.clone(), txn_id.clone());
                let mut last_emit: Option<std::time::Instant> = None;
                client
                    .send_attachment(
                        &room_id,
                        &path,
                        mime_type.as_deref(),
                        Some(&txn_id),
                        move |sent, total| {
                            // At most one event per interval, but never drop the final one.
                            let done = sent >= total;
                            let due = last_emit.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL);
                            if done || due {
                                last_emit = Some(std::time::Instant::now());
                                sink.emit(EngineEvent::UploadProgress {
                                    room_id: event_room.clone(),
                                    txn_id: event_txn.clone(),
                                    sent_bytes: sent,
                                    total_bytes: total,
                                });
                            }
                        },
                    )
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::AttachmentSent {
                    room_id,
                    txn_id,
                    event_id: None,
                })
            }
            EngineCommand::GetRoomMedia {
                room_id,
                kind,
                from,
                limit,
            } => {
                let client = ctx.active_client().await?;
                let (items, next_batch) = client
                    .room_media(&room_id, kind, from.as_deref(), limit)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::RoomMedia {
                    room_id,
                    items,
                    next_batch,
                })
            }
            EngineCommand::ClearMediaCache => {
                let freed_bytes = alloy_core::media::clear_media_cache(&ctx.data_dir)
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::MediaCacheCleared { freed_bytes })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Media group",
                name
            ))),
        }
    }
}
