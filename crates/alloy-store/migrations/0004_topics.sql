-- Alloy local store, schema version 4 (forum topics as chats nested under their forum).
--
-- Migrations are append-only; 0001 through 0003 stay byte-identical because their checksums are
-- recorded in `schema_migrations`.

-- The remote id of the forum a topic belongs to, NULL for every chat that is not a topic. A topic
-- is a chat of its own; the forum it sits in is only the grouping the sidebar nests it under.
ALTER TABLE chats ADD COLUMN parent_remote_chat_id TEXT;

-- Whether the chat is a forum container (its topics are the conversations, the container itself
-- holds no messages of its own in the UI).
ALTER TABLE chats ADD COLUMN is_forum INTEGER NOT NULL DEFAULT 0 CHECK (is_forum IN (0, 1));

-- The forum's topic list reads one account's topics of one forum, newest activity first.
CREATE INDEX idx_chats_parent
    ON chats (account_id, parent_remote_chat_id, last_activity_at DESC)
    WHERE parent_remote_chat_id IS NOT NULL;
