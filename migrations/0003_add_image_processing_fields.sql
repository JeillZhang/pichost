ALTER TABLE images
    ADD COLUMN thumbnail_key VARCHAR(512),
    ADD COLUMN thumbnail_url VARCHAR(1024),
    ADD COLUMN webp_key VARCHAR(512),
    ADD COLUMN webp_url VARCHAR(1024);
