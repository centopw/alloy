//! Embedded schema migrations with checksum verification.
//!
//! The applied history lives in `schema_migrations`. A database whose recorded checksum differs
//! from the embedded file is rejected instead of silently diverging, and a database written by a
//! newer binary is rejected instead of being downgraded.

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::error::StoreError;
use crate::model::now_ms;

/// Highest schema version this binary knows how to produce.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

impl Migration {
    fn checksum(&self) -> String {
        let digest = Sha256::digest(self.sql.as_bytes());
        format!("{digest:x}")
    }
}

/// Ordered, append-only. Add new migrations at the end; never edit a released file.
static MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "0001_init",
    sql: include_str!("../migrations/0001_init.sql"),
}];

/// Apply the PRAGMAs every connection needs. Call before any schema access.
pub(crate) fn configure(conn: &Connection) -> Result<(), StoreError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

/// Bring `conn` to the newest schema version and return it.
///
/// Each pending migration runs in its own transaction, so a failure leaves earlier versions
/// applied and recorded rather than a half-built schema.
pub fn migrate(conn: &mut Connection) -> Result<u32, StoreError> {
    configure(conn)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             checksum   TEXT NOT NULL,
             applied_at INTEGER NOT NULL
         )",
    )?;

    let applied = {
        let mut stmt =
            conn.prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let db_version = applied.last().map_or(0, |(version, _, _)| *version);
    let binary_version = MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
        .max(CURRENT_SCHEMA_VERSION);

    if db_version > binary_version {
        return Err(StoreError::SchemaTooNew {
            db: db_version,
            binary: binary_version,
        });
    }

    for (version, _name, checksum) in &applied {
        let Some(migration) = MIGRATIONS
            .iter()
            .find(|migration| migration.version == *version)
        else {
            return Err(StoreError::SchemaTooNew {
                db: db_version,
                binary: binary_version,
            });
        };
        if migration.checksum() != *checksum {
            return Err(StoreError::MigrationChecksumMismatch { version: *version });
        }
    }

    let mut current = db_version;
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > db_version)
    {
        let checksum = migration.checksum();
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![migration.version, migration.name, checksum, now_ms()],
        )?;
        tx.commit()?;
        current = migration.version;
    }

    Ok(current)
}

/// Newest applied version, 0 for a database without a history table.
pub(crate) fn current_version(conn: &mut Connection) -> Result<u32, StoreError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    Ok(conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<u32>>(0)
        })?
        .unwrap_or(0))
}
