-- Persist per-skenario 3D Skybox preset across sessions and computers
ALTER TABLE skenarios
    ADD COLUMN IF NOT EXISTS skybox TEXT NOT NULL DEFAULT 'day_clear';
