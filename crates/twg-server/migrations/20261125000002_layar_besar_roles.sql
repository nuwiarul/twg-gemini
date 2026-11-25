-- Add dedicated display-only roles: layar_besar (global) and layar_besar_satker (satker-scoped)

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_role_check;

ALTER TABLE users
    ADD CONSTRAINT users_role_check
    CHECK (role IN (
        'admin',
        'admin_monitor',
        'admin_satker',
        'user_satker',
        'user',
        'layar_besar',
        'layar_besar_satker'
    ));
