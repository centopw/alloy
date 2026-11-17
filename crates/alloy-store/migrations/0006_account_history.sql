-- Alloy local store, schema version 6: the encrypted-archive capability of one account.
--
-- Migrations are append-only; 0001 through 0005 stay byte-identical because their checksums are
-- recorded in `schema_migrations`.
--
-- Messenger reports whether the account has a PIN-protected encrypted history archive and whether
-- this device is enrolled in it inside the login `done` step. A launch that begins from the stored
-- session never logs in again, so the two flags are mirrored here: they are what the UI needs to
-- decide whether to offer the PIN, not the archive device itself, which lives only in the
-- connector's sealed session blob.
CREATE TABLE account_history (
    account_id TEXT PRIMARY KEY REFERENCES accounts (account_id) ON DELETE CASCADE,
    pin_backup INTEGER NOT NULL CHECK (pin_backup IN (0, 1)),
    enrolled   INTEGER NOT NULL CHECK (enrolled IN (0, 1)),
    updated_at INTEGER NOT NULL
) STRICT;
