-- Play Skenario interaction behavior per 3D model category + built-in system category protection.
-- Values for play_behavior:
--   'static_obstacle' : non-selectable, stationary building/prop acting as navigation obstacle
--   'gizmo_move'      : selectable in Play Skenario, repositioned via 3D Move Gizmo (X/Y/Z + Free XZ)
--   'rts_unit'        : single/box-selectable unit moving via right-click in formation with obstacle avoidance

ALTER TABLE model_categories
    ADD COLUMN IF NOT EXISTS play_behavior TEXT NOT NULL DEFAULT 'rts_unit'
        CHECK (play_behavior IN ('static_obstacle', 'gizmo_move', 'rts_unit'));

ALTER TABLE model_categories
    ADD COLUMN IF NOT EXISTS is_system BOOLEAN NOT NULL DEFAULT FALSE;

-- Lock the 7 built-in categories so they cannot be deleted, and assign their default Play Skenario behaviors
UPDATE model_categories
SET is_system = TRUE,
    play_behavior = CASE slug
        WHEN 'bangunans' THEN 'static_obstacle'
        WHEN 'tools'     THEN 'gizmo_move'
        ELSE 'rts_unit'
    END
WHERE slug IN (
    'bangunans',
    'kendaraan_umums',
    'kendaraans',
    'peoples',
    'stackholders',
    'tools',
    'units'
);
