//! Change feed used by UI projections now and by `alloy-gateway` sync later.

use rusqlite::Connection;

use crate::Store;
use crate::error::StoreError;
use crate::model::{EventRecord, NewEvent, now_ms};
use crate::rows;

impl Store {
    /// Append an event on its own. Mutating repository methods append inside their transaction
    /// instead, so a reader never observes a write without its event.
    pub async fn append_event(&self, event: NewEvent) -> Result<i64, StoreError> {
        self.with_conn(move |conn| {
            append(
                conn,
                &event.account_id,
                event.chat_id.as_deref(),
                &event.kind,
                event.payload,
                now_ms(),
            )
        })
        .await
    }

    /// Events with `seq` strictly greater than `since`, oldest first.
    pub async fn events_since(
        &self,
        since: i64,
        limit: u32,
    ) -> Result<Vec<EventRecord>, StoreError> {
        self.with_conn(move |conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM event_log WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2")?;
            let rows = stmt.query_map(rusqlite::params![since, limit], rows::event)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Newest `seq` in the change feed, or 0 before the first event. The engine records it when a
    /// projection is built so it can replay only what happened afterwards.
    pub async fn latest_event_seq(&self) -> Result<i64, StoreError> {
        self.with_conn(|conn| {
            Ok(
                conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM event_log", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .await
    }
}

pub(crate) fn append(
    conn: &Connection,
    account_id: &str,
    chat_id: Option<&str>,
    kind: &str,
    payload: serde_json::Value,
    created_at: i64,
) -> Result<i64, StoreError> {
    let payload = serde_json::to_string(&payload)?;
    conn.execute(
        "INSERT INTO event_log (account_id, chat_id, kind, payload, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![account_id, chat_id, kind, payload, created_at],
    )?;
    Ok(conn.last_insert_rowid())
}
