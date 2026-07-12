CREATE TABLE upload_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    image_id UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    task_type VARCHAR(32) NOT NULL DEFAULT 'all',
    payload JSONB,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_upload_tasks_image_id ON upload_tasks(image_id);
CREATE INDEX idx_upload_tasks_status ON upload_tasks(status);
