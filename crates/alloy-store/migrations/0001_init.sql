-- Alloy local store, schema version 1 (M0 contract, issue #127).
--
-- Data is owned by the device: every row is either per-account state, a projection of a remote
-- network object, or a local operation waiting to be pushed. Timestamps are milliseconds since the
-- Unix epoch (UTC); ids are opaque TEXT (UUID v4 for local ids, network-provided for remote ids).
--
-- The column types stay within INTEGER/TEXT/BLOB so the same schema can be replayed on Postgres by
-- the future alloy-gateway (M6). STRICT is only a SQLite-side guard against type drift.
--
-- Migrations are append-only: schema changes ship as a new numbered file plus a new entry in
-- `MIGRATIONS` in src/migrate.rs. Never edit this file once released - the runner compares the
-- SHA-256 of the embedded SQL with the checksum recorded in schema_migrations and refuses to open
-- a database whose history no longer matches.

CREATE TABLE accounts (
    account_id      TEXT PRIMARY KEY,
    network         TEXT NOT NULL CHECK (network IN ('telegram', 'messenger', 'fake')),
    remote_user_id  TEXT,
    display_name    TEXT,
    avatar_media_id TEXT,
    state           TEXT NOT NULL CHECK (state IN ('pending', 'active', 'needs_relogin', 'disabled')),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    -- A pending account has no remote id yet; SQLite (like Postgres) allows repeated NULLs here.
    UNIQUE (network, remote_user_id)
) STRICT;

-- Connector session material (tokens, cookies, device keys) sealed by the store. The connector
-- hands over opaque bytes and never sees the database.
CREATE TABLE account_secrets (
    account_id  TEXT PRIMARY KEY REFERENCES accounts (account_id) ON DELETE CASCADE,
    nonce       BLOB NOT NULL,
    ciphertext  BLOB NOT NULL,
    key_version INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- Per-account sync cursor. `scope` is 'global' or a remote_chat_id for per-chat backfill.
CREATE TABLE sync_state (
    account_id TEXT NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    scope      TEXT NOT NULL,
    cursor     TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, scope)
) STRICT;

CREATE TABLE chats (
    chat_id          TEXT PRIMARY KEY,
    account_id       TEXT NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    remote_chat_id   TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('direct', 'group', 'channel')),
    title            TEXT,
    avatar_media_id  TEXT,
    last_message_id  TEXT,
    last_activity_at INTEGER,
    unread_count     INTEGER NOT NULL DEFAULT 0,
    mention_count    INTEGER NOT NULL DEFAULT 0,
    is_pinned        INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1)),
    is_archived      INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    is_muted         INTEGER NOT NULL DEFAULT 0 CHECK (is_muted IN (0, 1)),
    muted_until      INTEGER,
    is_marked_unread INTEGER NOT NULL DEFAULT 0 CHECK (is_marked_unread IN (0, 1)),
    draft            TEXT,
    local_tags       TEXT,
    note             TEXT,
    UNIQUE (account_id, remote_chat_id)
) STRICT;

CREATE INDEX idx_chats_account_activity
    ON chats (account_id, is_archived, last_activity_at DESC);

CREATE TABLE participants (
    chat_id         TEXT NOT NULL REFERENCES chats (chat_id) ON DELETE CASCADE,
    remote_user_id  TEXT NOT NULL,
    display_name    TEXT,
    avatar_media_id TEXT,
    role            TEXT,
    PRIMARY KEY (chat_id, remote_user_id)
) STRICT;

CREATE TABLE messages (
    -- Stable integer key (alias of rowid) for the FTS external-content index. Message identity on
    -- the wire is `message_id`; `seq` never leaves the store. Without this, VACUUM could renumber
    -- rowids and silently misalign messages_fts.
    seq                 INTEGER PRIMARY KEY,
    message_id          TEXT NOT NULL UNIQUE,
    chat_id             TEXT NOT NULL REFERENCES chats (chat_id) ON DELETE CASCADE,
    remote_msg_id       TEXT,
    txn_id              TEXT UNIQUE,
    sender_remote_id    TEXT NOT NULL,
    is_own              INTEGER NOT NULL DEFAULT 0 CHECK (is_own IN (0, 1)),
    sent_at             INTEGER NOT NULL,
    edited_at           INTEGER,
    deleted_at          INTEGER,
    kind                TEXT NOT NULL CHECK (kind IN ('text', 'image', 'video', 'audio', 'voice',
                                                     'file', 'sticker', 'location', 'system')),
    body                TEXT,
    formatted           TEXT,
    reply_to_message_id TEXT,
    media_id            TEXT,
    send_state          TEXT CHECK (send_state IN ('queued', 'sending', 'sent', 'delivered', 'read',
                                                   'failed')),
    extra               TEXT,
    -- Local messages have no remote id yet; repeated NULLs are allowed by the unique index.
    UNIQUE (chat_id, remote_msg_id)
) STRICT;

CREATE INDEX idx_messages_chat_sent
    ON messages (chat_id, sent_at DESC, message_id);

CREATE TABLE reactions (
    message_id      TEXT NOT NULL REFERENCES messages (message_id) ON DELETE CASCADE,
    sender_remote_id TEXT NOT NULL,
    emoji           TEXT NOT NULL,
    reacted_at      INTEGER NOT NULL,
    PRIMARY KEY (message_id, sender_remote_id, emoji)
) STRICT;

CREATE TABLE read_markers (
    chat_id            TEXT NOT NULL REFERENCES chats (chat_id) ON DELETE CASCADE,
    remote_user_id     TEXT NOT NULL,
    up_to_message_id   TEXT NOT NULL,
    read_at            INTEGER NOT NULL,
    PRIMARY KEY (chat_id, remote_user_id)
) STRICT;

CREATE TABLE media (
    media_id    TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    remote_ref  TEXT NOT NULL,
    mime        TEXT,
    size_bytes  INTEGER,
    width       INTEGER,
    height      INTEGER,
    duration_ms INTEGER,
    file_name   TEXT,
    local_path  TEXT,
    sha256      TEXT,
    fetched_at  INTEGER
) STRICT;

CREATE TABLE outbox (
    txn_id          TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    chat_id         TEXT NOT NULL REFERENCES chats (chat_id) ON DELETE CASCADE,
    op              TEXT NOT NULL CHECK (op IN ('send', 'edit', 'delete', 'react', 'unreact',
                                                'read', 'typing')),
    payload         TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('pending', 'in_flight', 'done', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL,
    last_error      TEXT,
    created_at      INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_outbox_due ON outbox (state, next_attempt_at);

-- Monotonic change feed. The future alloy-gateway (M6) replays it by `seq` over WSS, so every
-- mutation that a UI projection or a synced client must observe appends a row in the same
-- transaction as the write.
CREATE TABLE event_log (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    chat_id    TEXT,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- Full-text search over message bodies. External-content FTS5 keeps the index out of the row
-- store; the triggers below mirror only non-NULL bodies and drop the old token set on edits.
CREATE VIRTUAL TABLE messages_fts USING fts5 (
    body,
    content = 'messages',
    content_rowid = 'seq'
);

CREATE TRIGGER messages_fts_after_insert AFTER INSERT ON messages
WHEN new.body IS NOT NULL
BEGIN
    INSERT INTO messages_fts (rowid, body) VALUES (new.seq, new.body);
END;

CREATE TRIGGER messages_fts_after_delete AFTER DELETE ON messages
WHEN old.body IS NOT NULL
BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, body) VALUES ('delete', old.seq, old.body);
END;

CREATE TRIGGER messages_fts_after_update AFTER UPDATE OF body ON messages
BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, body)
        SELECT 'delete', old.seq, old.body WHERE old.body IS NOT NULL;
    INSERT INTO messages_fts (rowid, body)
        SELECT new.seq, new.body WHERE new.body IS NOT NULL;
END;
