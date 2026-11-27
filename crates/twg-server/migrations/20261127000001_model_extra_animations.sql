-- Add JSONB column for extra named action animation clips on Animated 3D models
-- (e.g., waving, shooting, fainting, saluting).
ALTER TABLE model_assets
    ADD COLUMN IF NOT EXISTS extra_animations JSONB NOT NULL DEFAULT '[]'::jsonb;
