ALTER TABLE chat_members ADD COLUMN IF NOT EXISTS mute_until TIMESTAMPTZ NULL;
ALTER TABLE chat_members ADD COLUMN IF NOT EXISTS mute_forever BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE chat_members ADD COLUMN IF NOT EXISTS notify_type TEXT NOT NULL DEFAULT 'all' CHECK (notify_type IN ('all','mentions_only','none'));
ALTER TABLE chat_members ADD COLUMN IF NOT EXISTS mention_message_ids UUID[] NOT NULL DEFAULT '{}';
