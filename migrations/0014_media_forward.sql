CREATE TABLE IF NOT EXISTS sticker_packs (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    short_name TEXT NOT NULL UNIQUE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_public BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS stickers (
    id UUID PRIMARY KEY,
    pack_id UUID NOT NULL REFERENCES sticker_packs(id) ON DELETE CASCADE,
    emoji TEXT NULL,
    file_id UUID NOT NULL REFERENCES storage_files(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_sticker_packs (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pack_id UUID NOT NULL REFERENCES sticker_packs(id) ON DELETE CASCADE,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, pack_id)
);

CREATE INDEX IF NOT EXISTS idx_stickers_pack ON stickers (pack_id);
CREATE INDEX IF NOT EXISTS idx_user_sticker_packs_user ON user_sticker_packs (user_id);

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'text' CHECK (kind IN ('text','sticker','gif')),
    ADD COLUMN IF NOT EXISTS sticker_id UUID NULL REFERENCES stickers(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS gif_id TEXT NULL,
    ADD COLUMN IF NOT EXISTS gif_url TEXT NULL,
    ADD COLUMN IF NOT EXISTS gif_preview_url TEXT NULL,
    ADD COLUMN IF NOT EXISTS gif_provider TEXT NULL,
    ADD COLUMN IF NOT EXISTS forward_from_message_id UUID NULL REFERENCES messages(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS forward_from_chat_id UUID NULL REFERENCES chats(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS forward_from_sender_id UUID NULL REFERENCES users(id) ON DELETE SET NULL;
