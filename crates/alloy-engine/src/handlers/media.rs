use super::rooms::map_err;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_protocol::{EngineCommand, EngineResponse, MediaSource};
use std::path::PathBuf;

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
                MediaSource::Local { file_path } => {
                    if !PathBuf::from(&file_path).is_file() {
                        return Err(EngineError::not_found(format!("file {file_path}")));
                    }
                    Ok(EngineResponse::MediaRetrieved {
                        local_path: file_path,
                    })
                }
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
                if !path.is_file() {
                    return Err(EngineError::not_found(format!("file {file_path}")));
                }
                let client = ctx.active_client().await?;
                client
                    .send_attachment(&room_id, &path, mime_type.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::AttachmentSent {
                    room_id,
                    txn_id: txn_id.unwrap_or_default(),
                    event_id: None,
                })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Media group",
                name
            ))),
        }
    }
}
