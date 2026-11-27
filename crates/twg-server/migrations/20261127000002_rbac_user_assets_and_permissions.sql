-- Per-user 3D Model Asset entitlements (managed by admin, admin_monitor, and admin_satker for their satker)
CREATE TABLE IF NOT EXISTS user_model_assets (
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    model_asset_id UUID NOT NULL REFERENCES model_assets(id) ON DELETE CASCADE,
    assigned_by    UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, model_asset_id)
);

CREATE INDEX IF NOT EXISTS idx_user_model_assets_user ON user_model_assets(user_id);
CREATE INDEX IF NOT EXISTS idx_user_model_assets_model ON user_model_assets(model_asset_id);

-- Entity ownership (created_by) & multi-user approval delegation (allowed_user_ids)
ALTER TABLE markers
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE buildings
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE plot_lines
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE text_markers
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE anims
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb;
