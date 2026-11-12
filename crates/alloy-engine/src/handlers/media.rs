//! Media commands: the local cache and outbound attachments.

use std::path::{Path, PathBuf};

use alloy_connector::MediaRef;
use alloy_protocol::{EngineCommand, EngineResponse, MediaSource};
use alloy_store::{MediaRecord, MessageKind as StoreMessageKind};

use crate::context::EngineContext;
use crate::error::{EngineError, map_connector, map_store};
use crate::router::{CommandGroup, GroupHandler, command_name};
use crate::support::{self, OutboxAttachment, OutgoingDraft};

/// Handler for media commands (`CommandGroup::Media`).
pub struct MediaHandler;

impl MediaHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MediaHandler {
    fn default() -> Self {
        Self::new()
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
            // `kind` distinguishes thumbnails from full files for networks that expose both. The
            // connector contract carries one media reference, so both are served from the cache
            // entry the full file created.
            EngineCommand::GetMedia {
                source,
                kind: _kind,
                mime_type,
            } => get_media(ctx, &source, mime_type).await,
            EngineCommand::SendAttachment {
                room_id,
                file_path,
                mime_type,
                body,
                txn_id,
            } => send_attachment(ctx, &room_id, file_path, mime_type, body, txn_id).await,
            EngineCommand::SendVoice {
                room_id,
                file_path,
                duration_ms: _duration_ms,
                txn_id,
            } => send_voice(ctx, &room_id, file_path, txn_id).await,
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to the Media group",
                name
            ))),
        }
    }
}

async fn get_media(
    ctx: &EngineContext,
    source: &MediaSource,
    mime_type: Option<String>,
) -> Result<EngineResponse, EngineError> {
    let record = ctx
        .store
        .get_media(&source.media_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("media '{}'", source.media_id)))?;

    let cached = record
        .local_path
        .as_deref()
        .filter(|path| Path::new(*path).is_file());
    if let Some(path) = cached {
        return Ok(EngineResponse::Media {
            path: path.to_string(),
        });
    }

    let runtime = support::runtime_for(ctx, &record.account_id).await?;
    let extension = cache_extension(mime_type.as_deref().or(record.mime.as_deref()));
    let media_dir = ctx.media_dir();
    std::fs::create_dir_all(&media_dir).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot create the media cache {}: {err}",
            media_dir.display()
        ))
    })?;
    let destination = media_dir.join(format!("{}.{}", record.media_id, extension));
    let media = media_ref(&record, mime_type.as_deref());
    runtime
        .connector
        .fetch_media(&media, &destination)
        .await
        .map_err(map_connector)?;

    let path = destination.to_string_lossy().into_owned();
    ctx.store
        .set_media_local_path(
            &record.media_id,
            Some(path.clone()),
            None,
            Some(support::now_ms()),
        )
        .await
        .map_err(map_store)?;
    Ok(EngineResponse::Media { path })
}

async fn send_attachment(
    ctx: &EngineContext,
    room_id: &alloy_protocol::RoomId,
    file_path: String,
    mime_type: Option<String>,
    body: Option<String>,
    txn_id: Option<String>,
) -> Result<EngineResponse, EngineError> {
    let (chat, account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    let path = checked_file(ctx, &file_path)?;
    let metadata = std::fs::metadata(&path).map_err(|err| {
        EngineError::invalid_argument(format!("cannot read {}: {err}", path.display()))
    })?;
    if !metadata.is_file() {
        return Err(EngineError::invalid_argument(format!(
            "{} is not a file",
            path.display()
        )));
    }
    let too_large = runtime
        .capabilities
        .max_upload_bytes
        .filter(|max| metadata.len() > *max);
    if let Some(max) = too_large {
        return Err(EngineError::not_supported(format!(
            "attachments larger than {max} bytes"
        )));
    }

    let kind = support::kind_for_mime(mime_type.as_deref());
    let file_name = support::file_name_of(&file_path);
    let display_body = body.clone().or_else(|| file_name.clone());
    let draft = OutgoingDraft {
        kind,
        body: display_body,
        send_body: body,
        reply_to: None,
        txn_id,
        attachment: Some(OutboxAttachment {
            path: file_path,
            mime: mime_type,
            file_name,
            temporary: false,
        }),
    };
    let txn_id = support::queue_message(ctx, &chat, &account, draft).await?;
    Ok(EngineResponse::MessageQueued { txn_id })
}

async fn send_voice(
    ctx: &EngineContext,
    room_id: &alloy_protocol::RoomId,
    file_path: String,
    txn_id: Option<String>,
) -> Result<EngineResponse, EngineError> {
    let (chat, account, runtime) = support::chat_and_runtime(ctx, room_id).await?;
    if !runtime.capabilities.voice {
        return Err(EngineError::not_supported("voice"));
    }
    let source = checked_file(ctx, &file_path)?;
    let stored = copy_into_store(ctx, &source)?;
    let file_name = support::file_name_of(&file_path);
    let draft = OutgoingDraft {
        kind: StoreMessageKind::Voice,
        body: file_name.clone(),
        send_body: None,
        reply_to: None,
        txn_id,
        attachment: Some(OutboxAttachment {
            path: stored.to_string_lossy().into_owned(),
            mime: None,
            file_name,
            temporary: true,
        }),
    };
    let txn_id = support::queue_message(ctx, &chat, &account, draft).await?;
    Ok(EngineResponse::MessageQueued { txn_id })
}

/// Copies a recorded voice note into the engine's own directory, so the queue survives the user
/// moving or deleting the original before the upload happens.
fn copy_into_store(ctx: &EngineContext, source: &Path) -> Result<PathBuf, EngineError> {
    let directory = ctx.media_dir().join("voice");
    std::fs::create_dir_all(&directory).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot create the voice note directory {}: {err}",
            directory.display()
        ))
    })?;
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let destination = directory.join(format!("{}{extension}", uuid::Uuid::new_v4()));
    std::fs::copy(source, &destination).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot copy {} into the store: {err}",
            source.display()
        ))
    })?;
    Ok(destination)
}

/// Rejects a path the shell never handed to the engine, and a path that is not a file.
fn checked_file(ctx: &EngineContext, file_path: &str) -> Result<PathBuf, EngineError> {
    let path = PathBuf::from(file_path);
    if !ctx.attachment_path_allowed(&path) {
        return Err(EngineError::forbidden(
            "the attachment path is not allowed for this command",
        ));
    }
    if !path.exists() {
        return Err(EngineError::invalid_argument(format!(
            "{} does not exist",
            path.display()
        )));
    }
    Ok(path)
}

fn media_ref(record: &MediaRecord, mime_override: Option<&str>) -> MediaRef {
    MediaRef {
        remote_ref: record.remote_ref.clone(),
        mime: mime_override
            .map(str::to_string)
            .or_else(|| record.mime.clone())
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size_bytes: record
            .size_bytes
            .and_then(|value| u64::try_from(value).ok()),
        width: record.width.and_then(|value| u32::try_from(value).ok()),
        height: record.height.and_then(|value| u32::try_from(value).ok()),
        duration_ms: record
            .duration_ms
            .and_then(|value| u64::try_from(value).ok()),
        file_name: record.file_name.clone(),
    }
}

fn cache_extension(mime: Option<&str>) -> &'static str {
    match mime.unwrap_or_default() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/mpeg" => "mp3",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}
