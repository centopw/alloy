//! Chat participants as the network reports them.

use rusqlite::OptionalExtension;

use crate::Store;
use crate::error::StoreError;
use crate::events;
use crate::model::{ParticipantRecord, now_ms};
use crate::rows;

impl Store {
    /// Insert or refresh one participant. `None` keeps the stored value, so a partial update from
    /// the network (a name without an avatar) never erases what is known.
    pub async fn upsert_participant(
        &self,
        chat_id: &str,
        remote_user_id: &str,
        display_name: Option<String>,
        avatar_media_id: Option<String>,
        role: Option<String>,
    ) -> Result<ParticipantRecord, StoreError> {
        let chat_id = chat_id.to_owned();
        let remote_user_id = remote_user_id.to_owned();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let account_id = rows::account_for_chat(&tx, &chat_id)?;
            let record = tx.query_row(
                "INSERT INTO participants
                     (chat_id, remote_user_id, display_name, avatar_media_id, role)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (chat_id, remote_user_id) DO UPDATE SET
                     display_name = COALESCE(excluded.display_name, participants.display_name),
                     avatar_media_id =
                         COALESCE(excluded.avatar_media_id, participants.avatar_media_id),
                     role = COALESCE(excluded.role, participants.role)
                 RETURNING chat_id, remote_user_id, display_name, avatar_media_id, role",
                rusqlite::params![chat_id, remote_user_id, display_name, avatar_media_id, role],
                rows::participant,
            )?;
            events::append(
                &tx,
                &account_id,
                Some(record.chat_id.as_str()),
                "participant.upsert",
                serde_json::to_value(&record)?,
                now_ms(),
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    /// Members of one chat, named members first, then by name. Users without a display name sort
    /// last and ties break on the remote id, so the list is stable across calls.
    pub async fn list_participants(
        &self,
        chat_id: &str,
    ) -> Result<Vec<ParticipantRecord>, StoreError> {
        let chat_id = chat_id.to_owned();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT chat_id, remote_user_id, display_name, avatar_media_id, role
                 FROM participants
                 WHERE chat_id = ?1
                 ORDER BY display_name IS NULL, display_name COLLATE NOCASE ASC,
                          remote_user_id ASC",
            )?;
            let rows = stmt.query_map([chat_id], rows::participant)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    pub async fn get_participant(
        &self,
        chat_id: &str,
        remote_user_id: &str,
    ) -> Result<Option<ParticipantRecord>, StoreError> {
        let chat_id = chat_id.to_owned();
        let remote_user_id = remote_user_id.to_owned();
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT chat_id, remote_user_id, display_name, avatar_media_id, role
                     FROM participants
                     WHERE chat_id = ?1 AND remote_user_id = ?2",
                    rusqlite::params![chat_id, remote_user_id],
                    rows::participant,
                )
                .optional()?)
        })
        .await
    }
}
