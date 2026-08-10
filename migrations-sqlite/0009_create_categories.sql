-- migrations-sqlite/0009_create_categories.sql
CREATE TABLE categories (
    id          TEXT PRIMARY KEY DEFAULT (randomblob(16)),
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES categories(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(user_id, name, parent_id)
);
ALTER TABLE images
    ADD COLUMN category_id TEXT REFERENCES categories(id) ON DELETE SET NULL;
