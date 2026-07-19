-- migrations/0009_create_categories.sql

CREATE TABLE categories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        VARCHAR(128) NOT NULL,
    parent_id   UUID REFERENCES categories(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, name, parent_id)
);

ALTER TABLE images
    ADD COLUMN category_id UUID REFERENCES categories(id) ON DELETE SET NULL;
