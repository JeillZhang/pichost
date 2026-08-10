-- migrations-sqlite/0010_add_watermark_config.sql
ALTER TABLE users ADD COLUMN watermark_config TEXT;
