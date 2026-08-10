-- migrations-sqlite/0008_user_storage_configs.sql
CREATE TABLE user_storage_configs (
    id          TEXT PRIMARY KEY DEFAULT (randomblob(16)),
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    provider    TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0,
    config      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(user_id, name)
);
CREATE UNIQUE INDEX idx_default_per_user
    ON user_storage_configs(user_id) WHERE is_default = 1;
ALTER TABLE images
    ADD COLUMN storage_config_id TEXT REFERENCES user_storage_configs(id);
