-- Master Data: Preset Lokasi Cepat (Location Presets) + raise default max_zoom to 26

CREATE TABLE location_presets (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    center_lng  DOUBLE PRECISION NOT NULL,
    center_lat  DOUBLE PRECISION NOT NULL,
    zoom        DOUBLE PRECISION NOT NULL DEFAULT 16.0,
    min_zoom    DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    max_zoom    DOUBLE PRECISION NOT NULL DEFAULT 26.0,
    pitch       DOUBLE PRECISION NOT NULL DEFAULT 48.0,
    bearing     DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    map_mode    TEXT NOT NULL DEFAULT 'outdoor' CHECK (map_mode IN ('outdoor', 'indoor')),
    base_layer  TEXT NOT NULL DEFAULT 'osm',
    grid_size_m DOUBLE PRECISION NOT NULL DEFAULT 10.0,
    sort_order  INTEGER NOT NULL DEFAULT 100,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_location_presets_sort ON location_presets(sort_order ASC, name ASC);

INSERT INTO location_presets (
    name, center_lng, center_lat, zoom, min_zoom, max_zoom,
    pitch, bearing, map_mode, base_layer, grid_size_m, sort_order
)
VALUES
    ('Denpasar (Puputan)',       115.2167, -8.6705, 16.5, 3.0, 26.0, 48.0, 0.0, 'outdoor', 'osm',       10.0, 10),
    ('Jakarta (Monas)',          106.8272, -6.1754, 16.0, 3.0, 26.0, 48.0, 0.0, 'outdoor', 'osm',       10.0, 20),
    ('Surabaya (Tugu Pahlawan)', 112.7378, -7.2458, 16.0, 3.0, 26.0, 48.0, 0.0, 'outdoor', 'osm',       10.0, 30),
    ('IKN Nusantara',            116.7069, -0.9718, 15.5, 3.0, 26.0, 48.0, 0.0, 'outdoor', 'osm',       10.0, 40),
    ('Indoor Tactical Grid',       0.0,     0.0,    18.0, 3.0, 26.0, 48.0, 0.0, 'indoor',  'grid_dark', 10.0, 50)
ON CONFLICT (name) DO NOTHING;

ALTER TABLE skenarios ALTER COLUMN max_zoom SET DEFAULT 26.0;
