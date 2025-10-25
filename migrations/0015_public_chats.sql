ALTER TABLE chats
    ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS public_handle TEXT UNIQUE;

CREATE INDEX IF NOT EXISTS idx_chats_public_handle ON chats (is_public, public_handle);
