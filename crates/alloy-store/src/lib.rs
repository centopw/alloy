//! Local SQLite store for Alloy accounts, chats, messages and the outbound queue.
//!
//! The store owns all persistence and is backend-neutral in spirit: schema v1 keeps every column
//! to INTEGER/TEXT/BLOB so the same tables can later live on Postgres behind `alloy-gateway`.
//!
//! A single writer connection lives behind a `std::sync::Mutex`; async methods move the blocking
//! SQL onto `tokio::task::spawn_blocking`, so callers from the engine actor never block a runtime
//! worker. Connectors hand over opaque secret bytes - sealing happens here, and the connector never
//! touches the database.
//!
//! The store knows nothing about connector crates: `alloy-store` and `alloy-connector` are
//! independent, and the engine maps between them.

mod accounts;
mod chats;
mod crypto;
mod error;
mod events;
mod messages;
mod migrate;
mod model;
mod outbox;
mod rows;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

pub use error::StoreError;
pub use migrate::{CURRENT_SCHEMA_VERSION, migrate};
pub use model::{
    AccountRecord, AccountState, ChatFlagsUpdate, ChatKind, ChatRecord, ChatUpsert, EventRecord,
    MessageKind, MessageRecord, Network, NewAccount, NewEvent, NewMessage, NewOutboxItem,
    OutboxItem, OutboxOp, OutboxState, SendState,
};

/// Shared handle to the local database.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    secret_key: [u8; 32],
}

impl Store {
    /// Open (or create) the database at `path`, run the connection PRAGMAs and apply pending
    /// migrations. `secret_key` seals connector session material at rest; a wrong key only fails
    /// on the first `get_secret`, never on open.
    pub fn open(path: impl AsRef<Path>, secret_key: [u8; 32]) -> Result<Self, StoreError> {
        let mut conn = Connection::open(path)?;
        migrate(&mut conn)?;
        Ok(Self::from_connection(conn, secret_key))
    }

    /// Ephemeral database for tests and the fake connector.
    pub fn open_in_memory(secret_key: [u8; 32]) -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
        migrate(&mut conn)?;
        Ok(Self::from_connection(conn, secret_key))
    }

    fn from_connection(conn: Connection, secret_key: [u8; 32]) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            secret_key,
        }
    }

    /// Newest applied schema version.
    pub async fn schema_version(&self) -> Result<u32, StoreError> {
        self.with_conn(migrate::current_version).await
    }

    async fn with_conn<T, F>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            f(&mut guard)
        })
        .await
        .map_err(|err| StoreError::Blocking(err.to_string()))?
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}
