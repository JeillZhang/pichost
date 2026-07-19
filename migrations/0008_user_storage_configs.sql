-- migrations/0008_user_storage_configs.sql

CREATE TABLE user_storage_configs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        VARCHAR(64) NOT NULL,
    provider    VARCHAR(16) NOT NULL,
    is_default  BOOLEAN NOT NULL DEFAULT false,
    config      JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id, name)
);

CREATE UNIQUE INDEX idx_default_per_user
    ON user_storage_configs(user_id) WHERE is_default = true;

ALTER TABLE images
    ADD COLUMN storage_config_id UUID
    REFERENCES user_storage_configs(id);
