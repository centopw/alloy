//! Durable outbound queue. Rows move pending -> in_flight -> done (or back to pending with a
//! later `next_attempt_at` and one more attempt recorded).

use uuid::Uuid;

use crate::Store;
use crate::error::StoreError;
use crate::model::{NewOutboxItem, OutboxItem, OutboxState};
use crate::rows;

impl Store {
    /// Queue an operation. Returns the `txn_id` shared with the local message placeholder.
    pub async fn enqueue_outbox(&self, item: NewOutboxItem) -> Result<String, StoreError> {
        let txn_id = item.txn_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        self.with_conn(move |conn| {
            let payload = serde_json::to_string(&item.payload)?;
            conn.execute(
                "INSERT INTO outbox
                     (txn_id, account_id, chat_id, op, payload, state, attempts, next_attempt_at,
                      last_error, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, NULL, ?8)",
                rusqlite::params![
                    txn_id,
                    item.account_id,
                    item.chat_id,
                    item.op,
                    payload,
                    OutboxState::Pending,
                    item.next_attempt_at,
                    item.created_at
                ],
            )?;
            Ok(txn_id)
        })
        .await
    }

    /// Atomically claim up to `limit` rows that are due at `now`, moving them to `in_flight`.
    /// A claimed row is not returned again until it is failed (which returns it to `pending`).
    pub async fn claim_due_outbox(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<Vec<OutboxItem>, StoreError> {
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let claimed = {
                let mut stmt = tx.prepare(
                    "UPDATE outbox SET state = 'in_flight'
                     WHERE txn_id IN (
                         SELECT txn_id FROM outbox
                         WHERE state = 'pending' AND next_attempt_at <= ?1
                         ORDER BY next_attempt_at ASC, created_at ASC
                         LIMIT ?2
                     )
                     RETURNING txn_id, account_id, chat_id, op, payload, state, attempts,
                               next_attempt_at, last_error, created_at",
                )?;
                let mut rows = stmt.query(rusqlite::params![now, limit])?;
                let mut items = Vec::new();
                while let Some(row) = rows.next()? {
                    items.push(rows::outbox(row)?);
                }
                items
            };
            tx.commit()?;
            Ok(claimed)
        })
        .await
    }

    pub async fn complete_outbox(&self, txn_id: &str) -> Result<(), StoreError> {
        let txn_id = txn_id.to_owned();
        self.with_conn(move |conn| {
            let updated = conn.execute(
                "UPDATE outbox SET state = ?2, last_error = NULL WHERE txn_id = ?1",
                rusqlite::params![txn_id, OutboxState::Done],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("outbox item"));
            }
            Ok(())
        })
        .await
    }

    /// Record a failed attempt and schedule the retry. `attempts` is incremented by the database.
    pub async fn fail_outbox(
        &self,
        txn_id: &str,
        error: &str,
        next_attempt_at: i64,
    ) -> Result<(), StoreError> {
        let txn_id = txn_id.to_owned();
        let error = error.to_owned();
        self.with_conn(move |conn| {
            let updated = conn.execute(
                "UPDATE outbox
                 SET state = ?2,
                     attempts = attempts + 1,
                     last_error = ?3,
                     next_attempt_at = ?4
                 WHERE txn_id = ?1",
                rusqlite::params![txn_id, OutboxState::Pending, error, next_attempt_at],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound("outbox item"));
            }
            Ok(())
        })
        .await
    }
}
