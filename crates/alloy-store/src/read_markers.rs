//! Per-participant read positions.

use rusqlite::OptionalExtension;

use crate::Store;
use crate::error::StoreError;
use crate::events;
use crate::model::{ReadMarkerRecord, now_ms};
use crate::rows;

impl Store {
    /// Advance one participant's read position in a chat. Markers only move forward in time: an
    /// out-of-order older update is ignored without an event instead of rewinding the position
    /// the timeline shows.
    pub async fn set_read_marker(
        &self,
        chat_id: &str,
        remote_user_id: &str,
        up_to_message_id: &str,
        read_at: i64,
    ) -> Result<ReadMarkerRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        let remote_user_id = remote_user_id.to_owned();
        let up_to_message_id = up_to_message_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let account_id = rows::account_for_chat(&tx, &chat_id)?;
            let advanced = tx
                .query_row(
                    "INSERT INTO read_markers
                         (chat_id, remote_user_id, up_to_message_id, read_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (chat_id, remote_user_id) DO UPDATE SET
                         up_to_message_id = excluded.up_to_message_id,
                         read_at = excluded.read_at
                     WHERE excluded.read_at >= read_markers.read_at
                     RETURNING chat_id, remote_user_id, up_to_message_id, read_at",
                    rusqlite::params![&chat_id, &remote_user_id, up_to_message_id, read_at],
                    rows::read_marker,
                )
                .optional()?;
            let record = match advanced {
                Some(record) => {
                    events::append(
                        &tx,
                        &account_id,
                        Some(record.chat_id.as_str()),
                        "chat.read_marker",
                        serde_json::to_value(&record)?,
                        now_ms(),
                    )?;
                    record
                }
                // The stored marker is already newer; report it without emitting an event.
                None => tx.query_row(
                    "SELECT chat_id, remote_user_id, up_to_message_id, read_at
                     FROM read_markers WHERE chat_id = ?1 AND remote_user_id = ?2",
                    rusqlite::params![&chat_id, &remote_user_id],
                    rows::read_marker,
                )?,
            };
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    pub async fn list_read_markers(
        &self,
        chat_id: &str,
    ) -> Result<Vec<ReadMarkerRecord>, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT chat_id, remote_user_id, up_to_message_id, read_at
                 FROM read_markers
                 WHERE chat_id = ?1
                 ORDER BY read_at DESC, remote_user_id ASC",
            )?;
            let rows = stmt.query_map([chat_id], rows::read_marker)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }
}
