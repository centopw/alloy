//! Authenticated media download with an on-disk cache, and attachment upload.
//!
//! Owned by stream core-media.

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::RoomId;
use matrix_sdk::media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings};
use matrix_sdk::ruma::events::room::{EncryptedFile, MediaSource};
use matrix_sdk::ruma::{OwnedMxcUri, UInt};
use std::path::{Path, PathBuf};

fn sdk<E: std::fmt::Display>(e: E) -> AlloyError {
    AlloyError::MatrixSdk(e.to_string())
}

/// Parse the `uri` string carried by `TimelineItemContent::Media`: an `mxc://` URI for plain
/// media or the JSON of an `EncryptedFile`.
fn parse_source(uri: &str) -> AlloyResult<MediaSource> {
    if uri.trim_start().starts_with('{') {
        let file: EncryptedFile = serde_json::from_str(uri)
            .map_err(|e| AlloyError::MatrixSdk(format!("bad encrypted media source: {e}")))?;
        Ok(MediaSource::Encrypted(Box::new(file)))
    } else {
        let mxc = OwnedMxcUri::from(uri);
        if !mxc.is_valid() {
            return Err(AlloyError::MatrixSdk(format!("invalid mxc uri: {uri}")));
        }
        Ok(MediaSource::Plain(mxc))
    }
}

fn extension_for(mime: Option<&str>) -> &'static str {
    match mime.map(|m| m.split(';').next().unwrap_or(m).trim()) {
        Some("image/png") => "png",
        Some("image/jpeg") | Some("image/jpg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/avif") => "avif",
        Some("image/svg+xml") => "svg",
        Some("video/mp4") => "mp4",
        Some("video/webm") => "webm",
        Some("audio/mpeg") => "mp3",
        Some("audio/ogg") | Some("audio/opus") => "ogg",
        Some("audio/mp4") | Some("audio/aac") => "m4a",
        Some("audio/wav") | Some("audio/x-wav") => "wav",
        Some("application/pdf") => "pdf",
        Some("text/plain") => "txt",
        _ => "bin",
    }
}

fn cache_name(key: &str, ext: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("{hex}.{ext}")
}

/// Create `dir` and missing parents, readable only by the current user on unix.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Delete the on-disk media cache under `data_dir` and return the number of bytes freed. Cached
/// media is decrypted and the cache is shared by every account on this device, so logout clears
/// it. A missing cache frees 0 bytes.
pub async fn clear_media_cache(data_dir: &Path) -> AlloyResult<u64> {
    let dir = data_dir.join("media_cache");
    tokio::task::spawn_blocking(move || -> std::io::Result<u64> {
        let freed = dir_size(&dir)?;
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(freed),
        }
    })
    .await
    .map_err(std::io::Error::other)?
    .map_err(Into::into)
}

/// Total size in bytes of the files under `dir`. Symlinks count as their own size and are not
/// followed; a missing directory counts as 0.
fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut total = 0;
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        total += if metadata.is_dir() {
            dir_size(&entry.path())?
        } else {
            metadata.len()
        };
    }
    Ok(total)
}

impl AlloyClient {
    /// Download media (or a server-side thumbnail) into `cache_dir` and return the local path.
    ///
    /// Encrypted sources are decrypted; thumbnails of encrypted media fall back to the full
    /// file because servers cannot scale ciphertext. Results are cached by source + format.
    pub async fn fetch_media(
        &self,
        uri: &str,
        mime_type: Option<&str>,
        thumbnail: Option<(u32, u32)>,
        cache_dir: &Path,
    ) -> AlloyResult<PathBuf> {
        let source = parse_source(uri)?;
        let encrypted = matches!(source, MediaSource::Encrypted(_));
        let (format, format_key) = match thumbnail {
            Some((w, h)) if !encrypted => (
                MediaFormat::Thumbnail(MediaThumbnailSettings::new(UInt::from(w), UInt::from(h))),
                format!("thumb-{w}x{h}"),
            ),
            _ => (MediaFormat::File, "file".to_owned()),
        };
        let path = cache_dir.join(cache_name(
            &format!("{uri}|{format_key}"),
            extension_for(mime_type),
        ));
        if tokio::fs::try_exists(&path).await? {
            return Ok(path);
        }
        let bytes = self
            .client
            .media()
            .get_media_content(&MediaRequestParameters { source, format }, true)
            .await
            .map_err(sdk)?;
        let tmp = path.with_extension("part");
        let (dir, target) = (cache_dir.to_owned(), path.clone());
        // Written atomically so a crash never leaves a truncated cache entry. The directory and the
        // files are owner-only because the cache holds decrypted media.
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            create_private_dir(&dir)?;
            // A `.part` file left by a crash would make the exclusive create fail.
            let _ = std::fs::remove_file(&tmp);
            alloy_storage::write_private(&tmp, &bytes)?;
            std::fs::rename(&tmp, &target)
        })
        .await
        .map_err(std::io::Error::other)??;
        Ok(path)
    }

    /// Upload a local file as a message attachment (encrypted automatically in encrypted rooms).
    /// Returns when the upload and send finished. `on_progress(sent, total)` is called with the
    /// bytes of the request body transferred so far while the upload runs.
    pub async fn send_attachment(
        &self,
        room_id: &RoomId,
        file_path: &Path,
        mime_type: Option<&str>,
        txn_id: Option<&str>,
        mut on_progress: impl FnMut(u64, u64) + Send + 'static,
    ) -> AlloyResult<()> {
        let mime: mime::Mime = match mime_type {
            Some(m) => m
                .parse()
                .map_err(|e| AlloyError::MatrixSdk(format!("bad mime type {m}: {e}")))?,
            None => mime_guess_from_path(file_path),
        };
        self.ensure_encryption_state(room_id).await?;
        let timeline = self.cached_timeline(room_id).await?;
        let mut config = matrix_sdk_ui::timeline::AttachmentConfig::default();
        if let Some(txn) = txn_id {
            config.txn_id = Some(matrix_sdk::ruma::OwnedTransactionId::from(txn));
        }
        let upload = timeline.send_attachment(file_path, mime, config);
        let mut progress = upload.subscribe_to_send_progress();
        let forward = tokio::spawn(async move {
            while let Some(p) = progress.next().await {
                on_progress(p.current as u64, p.total as u64);
            }
        });
        let result = upload.await.map_err(sdk);
        // The progress stream ends once the upload future (and its observable) is dropped.
        let _ = forward.await;
        result
    }
}

pub(crate) fn mime_guess_from_path(path: &Path) -> mime::Mime {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => mime::IMAGE_PNG,
        Some("jpg") | Some("jpeg") => mime::IMAGE_JPEG,
        Some("gif") => mime::IMAGE_GIF,
        Some("svg") => mime::IMAGE_SVG,
        Some("pdf") => mime::APPLICATION_PDF,
        Some("txt") => mime::TEXT_PLAIN,
        Some("mp3") => "audio/mpeg".parse().expect("static mime"),
        Some("mp4") => "video/mp4".parse().expect("static mime"),
        Some("webm") => "video/webm".parse().expect("static mime"),
        Some("webp") => "image/webp".parse().expect("static mime"),
        _ => mime::APPLICATION_OCTET_STREAM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_encrypted_sources_parse() {
        assert!(matches!(
            parse_source("mxc://example.org/abc").unwrap(),
            MediaSource::Plain(_)
        ));
        assert!(parse_source("not-a-uri").is_err());
        assert!(parse_source("{\"broken\":").is_err());
    }

    #[test]
    fn cache_names_are_stable_distinct_and_typed() {
        let a = cache_name("mxc://x/a|file", "png");
        assert_eq!(a, cache_name("mxc://x/a|file", "png"));
        assert_ne!(a, cache_name("mxc://x/a|thumb-64x64", "png"));
        assert!(a.ends_with(".png"));
        assert_eq!(extension_for(Some("image/jpeg; charset=x")), "jpg");
        assert_eq!(extension_for(None), "bin");
    }
}
