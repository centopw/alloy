-- Alloy local store, schema version 2 (M1 engine surface, issue #129).
--
-- Adds only indexes: the engine resolves media by network reference, reads one chat's outbound
-- queue and looks read markers up per chat, none of which must scan a table. Migrations are
-- append-only; 0001 stays byte-identical because its checksum is recorded in `schema_migrations`.

-- A network media reference is unique per account. `insert_media` dedupes on this index and hands
-- back the existing `media_id`, so the same attachment is fetched and stored once per account.
CREATE UNIQUE INDEX idx_media_account_remote
    ON media (account_id, remote_ref);

-- The UI queue and CancelSend read one chat's unfinished rows, oldest first.
CREATE INDEX idx_outbox_chat_created
    ON outbox (chat_id, created_at);
