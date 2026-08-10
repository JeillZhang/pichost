-- migrations-sqlite/0003_add_image_processing_fields.sql
-- SQLite 仅支持单列 ADD COLUMN，须逐条执行
ALTER TABLE images ADD COLUMN thumbnail_key TEXT;
ALTER TABLE images ADD COLUMN thumbnail_url TEXT;
ALTER TABLE images ADD COLUMN webp_key TEXT;
ALTER TABLE images ADD COLUMN webp_url TEXT;
