ALTER TABLE chat_admin_permissions
    ADD COLUMN IF NOT EXISTS can_manage_members BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS granted_by UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS granted_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE TABLE IF NOT EXISTS member_mentions (
    chat_id UUID NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    excerpt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (chat_id, user_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_member_mentions_user ON member_mentions (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_member_mentions_message ON member_mentions (message_id);
