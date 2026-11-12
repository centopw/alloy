//! Attachment metadata and the on-disk cache columns.

use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::Store;
use crate::error::StoreError;
use crate::model::{MediaRecord, NewMedia};
use crate::rows;

impl Store {
    /// Insert a network media reference, or refresh the metadata of the row that already carries
    /// it. Identity is `(account_id, remote_ref)`, so two messages that reference the same file
    /// share one row and one download; the cache columns are never touched by an insert.
    pub async fn insert_media(&self, media: NewMedia) -> Result<String, StoreError> {
        self.with_conn(move |conn| {
            let media_id: String = conn.query_row(
                "INSERT INTO media
                     (media_id, account_id, remote_ref, mime, size_bytes, width, height,
                      duration_ms, file_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (account_id, remote_ref) DO UPDATE SET
                     mime = COALESCE(excluded.mime, media.mime),
                     size_bytes = COALESCE(excluded.size_bytes, media.size_bytes),
                     width = COALESCE(excluded.width, media.width),
                     height = COALESCE(excluded.height, media.height),
                     duration_ms = COALESCE(excluded.duration_ms, media.duration_ms),
                     file_name = COALESCE(excluded.file_name, media.file_name)
                 RETURNING media_id",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    media.account_id,
                    media.remote_ref,
                    media.mime,
                    media.size_bytes,
                    media.width,
                    media.height,
                    media.duration_ms,
                    media.file_name
                ],
                |row| row.get(0),
            )?;
            Ok(media_id)
        })
        .await
    }

    pub async fn get_media(&self, media_id: &str) -> Result<Option<MediaRecord>, StoreError> {
        let media_id = media_id.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT * FROM media WHERE media_id = ?1",
                    [media_id],
                    rows::media,
                )
                .optional()?)
        })
        .await
    }

    /// Record where a fetched file lives. The three columns describe the cache entry together, so
    /// they are written together: `None` clears the field (a re-fetch sets all three again).
    pub async fn set_media_local_path(
        &self,
        media_id: &str,
        local_path: Option<String>,
        sha256: Option<String>,
        fetched_at: Option<i64>,
    ) -> Result<MediaRecord, StoreError> {
        let media_id = media_id.to_owned();
        self.with_conn(move |conn| {
            let updated = conn.execute(
                "UPDATE media
                 SET local_path = ?2, sha256 = ?3, fetched_at = ?4
                 WHERE media_id = ?1",
                rusqlite::params![&media_id, local_path, sha256, fetched_at],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("media"));
            }
            Ok(conn.query_row(
                "SELECT * FROM media WHERE media_id = ?1",
                [&media_id],
                rows::media,
            )?)
        })
        .await
    }

    /// Forget every cached file path (of one account, or all of them) and return the paths that
    /// were set so the caller can delete the files. `sha256` and `fetched_at` describe the file,
    /// so they are cleared with it.
    pub async fn clear_media_local_paths(
        &self,
        account_id: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        let account_id = account_id.map(str::to_owned);
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let paths = {
                let mut stmt = tx.prepare(
                    "SELECT local_path FROM media
                     WHERE local_path IS NOT NULL AND (?1 IS NULL OR account_id = ?1)
                     ORDER BY media_id ASC",
                )?;
                let rows = stmt.query_map(rusqlite::params![&account_id], local_path)?;
                rows.collect::<rusqlite::Result<Vec<String>>>()?
            };
            tx.execute(
                "UPDATE media
                 SET local_path = NULL, sha256 = NULL, fetched_at = NULL
                 WHERE local_path IS NOT NULL AND (?1 IS NULL OR account_id = ?1)",
                rusqlite::params![&account_id],
            )?;
            tx.commit()?;
            Ok(paths)
        })
        .await
    }
}

fn local_path(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
    row.get(0)
}
