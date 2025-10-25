-- message edit/delete columns
ALTER TABLE messages ADD COLUMN IF NOT EXISTS edited_at TIMESTAMPTZ NULL;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS is_deleted BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;

-- optional chat metadata used by some handlers
ALTER TABLE chats ADD COLUMN IF NOT EXISTS chat_type TEXT;
ALTER TABLE chats ADD COLUMN IF NOT EXISTS owner_id UUID;
ALTER TABLE chats ADD COLUMN IF NOT EXISTS title TEXT;

-- small groups per-user reads (<=100 participants), purge by time
CREATE TABLE IF NOT EXISTS message_reads_small (
  message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  read_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (message_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_message_reads_small_at ON message_reads_small(read_at);

-- aggregate reads for DM and large groups (>100)
CREATE TABLE IF NOT EXISTS message_reads_agg (
  message_id UUID PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  is_read BOOLEAN NOT NULL DEFAULT FALSE,
  first_read_at TIMESTAMPTZ NULL
);

-- channel view counters
CREATE TABLE IF NOT EXISTS message_views (
  message_id UUID PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  views BIGINT NOT NULL DEFAULT 0,
  last_view_at TIMESTAMPTZ NULL
);
