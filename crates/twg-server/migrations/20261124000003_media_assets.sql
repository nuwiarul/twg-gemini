-- Media Assets table for Image Textures (symmetric + non-destructive resize), Sounds, and Videos

CREATE TABLE media_assets (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug                 TEXT NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    media_type           TEXT NOT NULL CHECK (media_type IN ('texture', 'sound', 'video')),
    category             TEXT NOT NULL DEFAULT 'general',
    mime_type            TEXT NOT NULL DEFAULT 'application/octet-stream',
    -- Original file (ALWAYS preserved! For texture: symmetric SxS original)
    original_path        TEXT NOT NULL,
    original_sha256      TEXT NOT NULL,
    original_size_bytes  BIGINT NOT NULL DEFAULT 0,
    original_width       INTEGER,
    original_height      INTEGER,
    -- Active file (for texture: equals original or downscaled symmetric TxT <= original_width)
    active_path          TEXT NOT NULL,
    active_sha256        TEXT NOT NULL,
    active_size_bytes    BIGINT NOT NULL DEFAULT 0,
    active_width         INTEGER,
    active_height        INTEGER,
    -- Optional thumbnail (auto-generated for video)
    thumbnail_path       TEXT,
    thumbnail_sha256     TEXT,
    thumbnail_size_bytes BIGINT NOT NULL DEFAULT 0,
    -- Media metadata
    duration_sec         DOUBLE PRECISION,
    crop_mode            TEXT,
    version              INTEGER NOT NULL DEFAULT 1,
    created_by           UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_media_assets_type ON media_assets(media_type);
CREATE INDEX idx_media_assets_updated ON media_assets(updated_at DESC);
