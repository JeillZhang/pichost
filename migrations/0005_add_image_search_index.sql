-- migrations/0005_add_image_search_index.sql
-- Add composite index for user-scoped filename search (ILIKE queries on original_name)
-- Index on (user_id, original_name) supports WHERE user_id = $1 AND original_name ILIKE $2
CREATE INDEX IF NOT EXISTS idx_images_user_filename ON images(user_id, original_name);
