-- migrations-sqlite/0006_add_storage_quota.sql
ALTER TABLE users ADD COLUMN storage_quota INTEGER;
