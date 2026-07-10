CREATE TABLE images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    public_key VARCHAR(16) UNIQUE NOT NULL,
    original_name VARCHAR(255) NOT NULL,
    storage_key VARCHAR(512) NOT NULL,
    storage_backend VARCHAR(32) NOT NULL,
    mime_type VARCHAR(128) NOT NULL,
    file_size BIGINT NOT NULL,
    width INTEGER,
    height INTEGER,
    sha256 VARCHAR(64) NOT NULL,
    url VARCHAR(1024) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_images_user_sha256 ON images(user_id, sha256);
