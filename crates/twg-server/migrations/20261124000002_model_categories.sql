-- Dynamic 3D model categories table (managed by Admin without rebuilding)

CREATE TABLE model_categories (
    slug               TEXT PRIMARY KEY CHECK (slug ~ '^[a-z0-9][a-z0-9_-]{0,62}$'),
    name               TEXT NOT NULL,
    supports_idle_walk BOOLEAN NOT NULL DEFAULT FALSE,
    default_scale      DOUBLE PRECISION NOT NULL DEFAULT 3.5,
    sort_order         INTEGER NOT NULL DEFAULT 100,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO model_categories (slug, name, supports_idle_walk, default_scale, sort_order)
VALUES
    ('bangunans',       'Bangunans',       FALSE, 3.0, 10),
    ('kendaraan_umums', 'Kendaraan Umums', FALSE, 3.5, 20),
    ('kendaraans',      'Kendaraans',      FALSE, 3.5, 30),
    ('peoples',         'Peoples',         TRUE,  5.0, 40),
    ('stackholders',    'Stackholders',    TRUE,  5.0, 50),
    ('tools',           'Tools',           FALSE, 3.5, 60),
    ('units',           'Units',           TRUE,  5.0, 70)
ON CONFLICT (slug) DO NOTHING;

-- Replace hardcoded CHECK constraint with FK to model_categories
ALTER TABLE model_assets DROP CONSTRAINT IF EXISTS model_assets_category_check;
ALTER TABLE model_assets
    ADD CONSTRAINT fk_model_assets_category
    FOREIGN KEY (category) REFERENCES model_categories(slug)
    ON UPDATE CASCADE ON DELETE RESTRICT;

CREATE INDEX idx_model_categories_sort ON model_categories(sort_order ASC, slug ASC);
