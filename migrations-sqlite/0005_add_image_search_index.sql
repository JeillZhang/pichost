-- migrations-sqlite/0005_add_image_search_index.sql
CREATE INDEX IF NOT EXISTS idx_images_user_filename ON images(user_id, original_name);
