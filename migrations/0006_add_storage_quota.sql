-- Add per-user storage quota (NULL = unlimited, bytes)
ALTER TABLE users ADD COLUMN IF NOT EXISTS storage_quota BIGINT;
COMMENT ON COLUMN users.storage_quota IS 'Per-user storage quota in bytes. NULL = unlimited.';
