-- Per-Operasi Imported Assets (3D Models, Image Textures, Sounds, Videos)
-- operasi_id IS NULL  => Master / Global Asset (managed by global Admin in Assets 3D)
-- operasi_id IS NOT NULL => Operasi-Scoped Imported Asset (uploaded in Plot Editor, shared across
--                           all scenarios & all roles within that Operasi, isolated from other Operasis)

ALTER TABLE model_assets
    ADD COLUMN IF NOT EXISTS operasi_id UUID REFERENCES operasis(id) ON DELETE CASCADE;

ALTER TABLE media_assets
    ADD COLUMN IF NOT EXISTS operasi_id UUID REFERENCES operasis(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_model_assets_operasi ON model_assets(operasi_id);
CREATE INDEX IF NOT EXISTS idx_media_assets_operasi ON media_assets(operasi_id);
