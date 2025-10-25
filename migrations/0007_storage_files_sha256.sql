ALTER TABLE storage_files ADD COLUMN sha256 TEXT NOT NULL;
ALTER TABLE storage_files ADD COLUMN size BIGINT NOT NULL;
ALTER TABLE storage_files DROP COLUMN user_id;
DROP INDEX IF EXISTS idx_storage_files_user;
CREATE UNIQUE INDEX IF NOT EXISTS idx_storage_files_sha256_unique ON storage_files(sha256);
