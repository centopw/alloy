-- Alloy local store, schema version 5 (tag-set stamp for cross-device merge).
--
-- Migrations are append-only; 0001 through 0004 stay byte-identical because their checksums are
-- recorded in `schema_migrations`.

-- When this chat's tag set last changed. A configuration bundle imports a chat's tags
-- last-write-wins per entity, and without a local stamp the exporting device cannot say how old
-- its copy is. Existing rows start at 0, which every bundle stamp is newer than.
ALTER TABLE chats ADD COLUMN tags_updated_at INTEGER NOT NULL DEFAULT 0;

-- The bundle export reads every chat that carries tags, archived or not.
CREATE INDEX idx_chats_tagged
    ON chats (tags_updated_at)
    WHERE local_tags IS NOT NULL AND local_tags != '[]';
