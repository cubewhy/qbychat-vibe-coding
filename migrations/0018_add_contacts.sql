-- Add contacts table for user relationships
CREATE TABLE IF NOT EXISTS contacts (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    contact_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('friend', 'blocked')),
    added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, contact_user_id)
);