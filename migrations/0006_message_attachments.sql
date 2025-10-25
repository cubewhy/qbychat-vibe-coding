CREATE TABLE IF NOT EXISTS message_attachments (
  message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  file_id UUID NOT NULL REFERENCES storage_files(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, file_id)
);
