-- Fresh, clean PostgreSQL 18 schema for Tactical Wall Game (Full Rust)

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE satkers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_by  UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    username    TEXT NOT NULL UNIQUE,
    password    TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('admin', 'admin_monitor', 'admin_satker', 'user_satker', 'user')),
    satker_id   UUID REFERENCES satkers(id) ON DELETE SET NULL,
    profile_img TEXT,
    units       JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_sessions (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id          TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL,
    is_single_device   BOOLEAN NOT NULL DEFAULT FALSE,
    last_used_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at         TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, device_id)
);

CREATE TABLE operasis (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    description TEXT,
    satker_id   UUID REFERENCES satkers(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE skenarios (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id    UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    satker_id     UUID REFERENCES satkers(id) ON DELETE SET NULL,
    name          TEXT NOT NULL,
    center_lng    DOUBLE PRECISION NOT NULL DEFAULT 115.2167,
    center_lat    DOUBLE PRECISION NOT NULL DEFAULT -8.6705,
    zoom          DOUBLE PRECISION NOT NULL DEFAULT 16.0,
    min_zoom      DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    max_zoom      DOUBLE PRECISION NOT NULL DEFAULT 20.0,
    pitch         DOUBLE PRECISION NOT NULL DEFAULT 45.0,
    bearing       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    map_mode      TEXT NOT NULL DEFAULT 'outdoor' CHECK (map_mode IN ('outdoor', 'indoor')),
    base_layer    TEXT NOT NULL DEFAULT 'osm',
    indoor_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE markers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id  UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    unit_id     TEXT NOT NULL,
    name        TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'unit',
    model_path  TEXT NOT NULL,
    quantity    INTEGER NOT NULL DEFAULT 1,
    lng         DOUBLE PRECISION NOT NULL,
    lat         DOUBLE PRECISION NOT NULL,
    altitude    DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    rot_x       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    rot_y       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    rot_z       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    scale_x     DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    scale_y     DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    scale_z     DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    is_spawn    BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE buildings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id  UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    color       TEXT NOT NULL DEFAULT '#38bdf8',
    height      DOUBLE PRECISION NOT NULL DEFAULT 24.0,
    geom        JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE plot_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id  UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#f59e0b',
    width       DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    alpha       DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    geom        JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE text_markers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id  UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    text        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#ffffff',
    font_size   DOUBLE PRECISION NOT NULL DEFAULT 14.0,
    lng         DOUBLE PRECISION NOT NULL,
    lat         DOUBLE PRECISION NOT NULL,
    altitude    DOUBLE PRECISION NOT NULL DEFAULT 12.0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE particles (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id    UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id   UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    particle_type TEXT NOT NULL,
    lng           DOUBLE PRECISION NOT NULL,
    lat           DOUBLE PRECISION NOT NULL,
    is_spawn      BOOLEAN NOT NULL DEFAULT TRUE,
    data          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE alurs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id  UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    payload     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE anims (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id   UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id  UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    marker_id    UUID REFERENCES markers(id) ON DELETE SET NULL,
    name         TEXT NOT NULL,
    anim_type    TEXT NOT NULL DEFAULT 'route',
    sort_order   INTEGER NOT NULL DEFAULT 0,
    speed        DOUBLE PRECISION NOT NULL DEFAULT 12.0,
    duration_sec DOUBLE PRECISION NOT NULL DEFAULT 20.0,
    replay       BOOLEAN NOT NULL DEFAULT FALSE,
    autoplay     BOOLEAN NOT NULL DEFAULT FALSE,
    geom         JSONB NOT NULL DEFAULT '[]'::jsonb,
    data         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE commands (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id    UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id   UUID NOT NULL REFERENCES skenarios(id) ON DELETE CASCADE,
    marker_id     UUID REFERENCES markers(id) ON DELETE SET NULL,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    command       TEXT NOT NULL,
    payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE game_session_snapshots (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operasi_id       UUID NOT NULL REFERENCES operasis(id) ON DELETE CASCADE,
    skenario_id      UUID NOT NULL UNIQUE REFERENCES skenarios(id) ON DELETE CASCADE,
    snapshot_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    map_state        JSONB NOT NULL DEFAULT '{}'::jsonb,
    markers_state    JSONB NOT NULL DEFAULT '[]'::jsonb,
    anims_state      JSONB NOT NULL DEFAULT '[]'::jsonb,
    version          INTEGER NOT NULL DEFAULT 1,
    updated_by       UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_satker ON users(satker_id);
CREATE INDEX idx_user_sessions_user_device ON user_sessions(user_id, device_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_operasis_satker ON operasis(satker_id);
CREATE INDEX idx_skenarios_operasi ON skenarios(operasi_id);
CREATE INDEX idx_markers_skenario ON markers(skenario_id);
CREATE INDEX idx_buildings_skenario ON buildings(skenario_id);
CREATE INDEX idx_plot_lines_skenario ON plot_lines(skenario_id);
CREATE INDEX idx_text_markers_skenario ON text_markers(skenario_id);
CREATE INDEX idx_particles_skenario ON particles(skenario_id);
CREATE INDEX idx_alurs_skenario ON alurs(skenario_id);
CREATE INDEX idx_anims_skenario ON anims(skenario_id, sort_order);
CREATE INDEX idx_commands_skenario ON commands(skenario_id, created_at DESC);
