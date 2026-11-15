-- Alloy local store, schema version 3 (pinned chats in a user-chosen order, pinned messages).
--
-- Migrations are append-only; 0001 and 0002 stay byte-identical because their checksums are
-- recorded in `schema_migrations`.

-- Position of a pinned chat among the account's pinned chats, 0 at the top. NULL for an
-- unpinned chat. The order is the user's (drag in the room list) or the network's, whichever
-- wrote last; unpinned chats keep ordering by activity, so this column never affects them.
ALTER TABLE chats ADD COLUMN pin_order INTEGER;

-- Whether the message is pinned in its chat. The network's pin state where the network has
-- pins, a local one otherwise.
ALTER TABLE messages ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1));

-- The pinned-message bar reads one chat's pinned messages, newest first.
CREATE INDEX idx_messages_chat_pinned
    ON messages (chat_id, sent_at DESC)
    WHERE is_pinned = 1;
