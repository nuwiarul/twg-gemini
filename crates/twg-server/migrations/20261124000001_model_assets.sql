-- 3D glTF/GLB Model Assets registry (LOD0 + LOD2 + Idle/Walk + SHA-256 offline sync)

CREATE TABLE model_assets (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug                 TEXT NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    category             TEXT NOT NULL CHECK (category IN (
        'bangunans', 'kendaraan_umums', 'kendaraans', 'peoples', 'stackholders', 'tools', 'units'
    )),
    model_type           TEXT NOT NULL CHECK (model_type IN ('static', 'animated')),
    -- Primary / Idle LOD0
    model_path           TEXT NOT NULL,
    model_sha256         TEXT NOT NULL,
    model_size_bytes     BIGINT NOT NULL DEFAULT 0,
    model_tris           INTEGER NOT NULL DEFAULT 0,
    -- Primary / Idle LOD2
    lod2_path            TEXT,
    lod2_sha256          TEXT,
    lod2_size_bytes      BIGINT NOT NULL DEFAULT 0,
    lod2_tris            INTEGER NOT NULL DEFAULT 0,
    -- Walk LOD0 (animated only)
    walk_path            TEXT,
    walk_sha256          TEXT,
    walk_size_bytes      BIGINT NOT NULL DEFAULT 0,
    walk_tris            INTEGER NOT NULL DEFAULT 0,
    -- Walk LOD2 (animated only)
    walk_lod2_path       TEXT,
    walk_lod2_sha256     TEXT,
    walk_lod2_size_bytes BIGINT NOT NULL DEFAULT 0,
    walk_lod2_tris       INTEGER NOT NULL DEFAULT 0,
    -- Default transform metadata
    default_height       DOUBLE PRECISION NOT NULL DEFAULT 10.0,
    default_rot_x        DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    default_rot_y        DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    default_rot_z        DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    version              INTEGER NOT NULL DEFAULT 1,
    created_by           UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_model_assets_category ON model_assets(category);
CREATE INDEX idx_model_assets_type ON model_assets(model_type);
CREATE INDEX idx_model_assets_updated ON model_assets(updated_at DESC);
