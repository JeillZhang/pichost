-- Add per-user watermark configuration (NULL = disabled, JSONB config)
ALTER TABLE users ADD COLUMN IF NOT EXISTS watermark_config JSONB;
COMMENT ON COLUMN users.watermark_config IS 'Per-user watermark configuration. NULL = watermark disabled. JSON schema: {enabled, text, font, font_size, color, rotation, scale, position, margin_x, margin_y}';
