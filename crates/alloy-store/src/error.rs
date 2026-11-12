//! Typed errors surfaced by the store. Library code returns these; only the app shell decides how
//! to present them.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A migration file changed after it was applied. Migration files are immutable.
    #[error("migration {version} checksum does not match the embedded schema")]
    MigrationChecksumMismatch { version: u32 },
    /// The database was written by a newer binary; opening it read-write would corrupt it.
    #[error("database schema version {db} is newer than this binary ({binary})")]
    SchemaTooNew { db: u32, binary: u32 },
    #[error("invalid {field}: {value}")]
    InvalidValue { field: &'static str, value: String },
    #[error("secret encryption failed")]
    Encrypt,
    /// Wrong device key, truncated nonce, or tampered ciphertext.
    #[error("secret decryption failed for account {account_id}")]
    Decrypt { account_id: String },
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("background store task failed: {0}")]
    Blocking(String),
}
