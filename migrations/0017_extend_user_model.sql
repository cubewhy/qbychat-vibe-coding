-- Add new fields to users table
ALTER TABLE users ADD COLUMN bio TEXT;
ALTER TABLE users ADD COLUMN is_online BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE users ADD COLUMN last_seen_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN online_status_visibility TEXT NOT NULL DEFAULT 'everyone';