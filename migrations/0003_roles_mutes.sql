CREATE TABLE IF NOT EXISTS chat_roles (
  chat_id UUID NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK (role IN ('admin')),
  PRIMARY KEY (chat_id, user_id, role)
);

CREATE TABLE IF NOT EXISTS chat_mutes (
  chat_id UUID NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  muted_until TIMESTAMPTZ NULL,
  PRIMARY KEY (chat_id, user_id)
);
