-- migrations-sqlite/0004_create_upload_tasks.sql
CREATE TABLE upload_tasks (
    id TEXT PRIMARY KEY DEFAULT (randomblob(16)),
    image_id TEXT NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL DEFAULT 'all',
    payload TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    completed_at TEXT
);
CREATE INDEX idx_upload_tasks_image_id ON upload_tasks(image_id);
CREATE INDEX idx_upload_tasks_status ON upload_tasks(status);
