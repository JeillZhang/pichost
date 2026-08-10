-- migrations-sqlite/0001_create_users.sql
CREATE TABLE users (
    id TEXT PRIMARY KEY DEFAULT (randomblob(16)),
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    storage_backend TEXT NOT NULL DEFAULT 'local',
    storage_prefix TEXT NOT NULL DEFAULT '',
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
