use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::password::hash_password;

pub async fn run_seed_if_empty(db: &PgPool) -> Result<()> {
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await?;
    if user_count > 0 {
        return Ok(());
    }

    tracing::info!(
        "Empty database detected — seeding initial Satker, users, and Denpasar 3D demo scenario..."
    );

    let satker_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    let operator_id = Uuid::new_v4();
    let operasi_id = Uuid::new_v4();
    let skenario_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO satkers (id, code, name, is_active, created_by) VALUES ($1, $2, $3, TRUE, $4)",
    )
    .bind(satker_id)
    .bind("POLDA-BALI")
    .bind("Polda Bali")
    .bind(admin_id)
    .execute(db)
    .await?;

    let admin_hash = hash_password("admin123")?;
    let operator_hash = hash_password("operator123")?;

    sqlx::query(
        r#"
        INSERT INTO users (id, name, username, password, role, satker_id, units)
        VALUES
          ($1, 'Commander Admin', 'admin', $2, 'admin', $3, '[]'::jsonb),
          ($4, 'Operator Lapangan Bali', 'operator', $5, 'user_satker', $3, '[]'::jsonb)
        "#,
    )
    .bind(admin_id)
    .bind(admin_hash)
    .bind(satker_id)
    .bind(operator_id)
    .bind(operator_hash)
    .execute(db)
    .await?;

    sqlx::query("INSERT INTO operasis (id, name, description, satker_id) VALUES ($1, $2, $3, $4)")
        .bind(operasi_id)
        .bind("Operasi Tactical Wall Bali 2026")
        .bind("Operasi pengamanan rute & sektor pusat kota Denpasar (Full Rust 3D)")
        .bind(satker_id)
        .execute(db)
        .await?;

    // Denpasar Lapangan Puputan Badung
    let center_lng = 115.2167_f64;
    let center_lat = -8.6705_f64;

    sqlx::query(
        r#"
        INSERT INTO skenarios (
            id, operasi_id, satker_id, name, center_lng, center_lat,
            zoom, min_zoom, max_zoom, pitch, bearing, map_mode, base_layer, indoor_config
        )
        VALUES ($1, $2, $3, $4, $5, $6, 16.5, 3.0, 20.0, 48.0, 0.0, 'outdoor', 'osm', '{"grid_size_m": 10.0}'::jsonb)
        "#,
    )
    .bind(skenario_id)
    .bind(operasi_id)
    .bind(satker_id)
    .bind("Pengamanan Sektor Denpasar (Demo 3D)")
    .bind(center_lng)
    .bind(center_lat)
    .execute(db)
    .await?;

    let patwal_id = Uuid::new_v4();
    let heli_id = Uuid::new_v4();
    let brimob_id = Uuid::new_v4();
    let pos_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO markers (
            id, operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
            lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z, is_spawn, description
        )
        VALUES
          ($1, $2, $3, 'patwal-01', 'Mobil Patwal 01', 'kendaraan', 'models/kendaraans/mobil_patwal_anim_1.glb', 1, 115.2160, -8.6710, 0.0, 0.0, 0.0, 0.0, 4.0, 4.0, 4.0, TRUE, 'Unit pengawalan rute utama'),
          ($4, $2, $3, 'heli-01', 'Heli Polisi 01', 'kendaraan', 'models/kendaraans/helikopter_polisi_anim_1.glb', 1, 115.2175, -8.6698, 45.0, 0.0, -35.0, 0.0, 3.5, 3.5, 3.5, TRUE, 'Pemantauan udara sektor'),
          ($5, $2, $3, 'brimob-01', 'Regu Brimob 01', 'unit', 'models/units/brimob_walk.glb', 1, 115.2168, -8.6707, 0.0, 0.0, 0.0, 0.0, 5.0, 5.0, 5.0, TRUE, 'Tim taktis lapangan'),
          ($6, $2, $3, 'pos-01', 'Pos Polisi Sektor', 'bangunan', 'models/bangunans/pos_polisi_1.glb', 1, 115.2156, -8.6701, 0.0, 0.0, 0.0, 0.0, 3.0, 3.0, 3.0, TRUE, 'Posko lapangan utama')
        "#,
    )
    .bind(patwal_id)
    .bind(operasi_id)
    .bind(skenario_id)
    .bind(heli_id)
    .bind(brimob_id)
    .bind(pos_id)
    .execute(db)
    .await?;

    // Extruded Posko Tactical Building polygon
    let building_geom = serde_json::json!([
        [115.2171, -8.6702],
        [115.2175, -8.6702],
        [115.2175, -8.6706],
        [115.2171, -8.6706]
    ]);
    sqlx::query(
        r#"
        INSERT INTO buildings (operasi_id, skenario_id, name, description, color, height, geom)
        VALUES ($1, $2, 'Pos Komando Utama', 'Gedung komando taktis 3D', '#38bdf8', 28.0, $3)
        "#,
    )
    .bind(operasi_id)
    .bind(skenario_id)
    .bind(building_geom)
    .execute(db)
    .await?;

    // Tactical perimeter polyline
    let line_geom = serde_json::json!([
        [115.2153, -8.6696],
        [115.2180, -8.6696],
        [115.2180, -8.6714],
        [115.2153, -8.6714],
        [115.2153, -8.6696]
    ]);
    sqlx::query(
        r#"
        INSERT INTO plot_lines (operasi_id, skenario_id, name, color, width, alpha, geom)
        VALUES ($1, $2, 'Ring Pengamanan Zona 1', '#f59e0b', 3.0, 0.95, $3)
        "#,
    )
    .bind(operasi_id)
    .bind(skenario_id)
    .bind(line_geom)
    .execute(db)
    .await?;

    // Text labels
    sqlx::query(
        r#"
        INSERT INTO text_markers (operasi_id, skenario_id, text, color, font_size, lng, lat, altitude)
        VALUES
          ($1, $2, 'POSKO UTAMA SEKTOR A', '#38bdf8', 15.0, 115.2173, -8.6704, 34.0),
          ($1, $2, 'TITIK KUMPUL PATWAL', '#fbbf24', 14.0, 115.2160, -8.6710, 14.0)
        "#,
    )
    .bind(operasi_id)
    .bind(skenario_id)
    .execute(db)
    .await?;

    // Demo route animation for Patwal 01
    let route_geom = serde_json::json!([
        [115.2160, -8.6710, 0.0],
        [115.2177, -8.6710, 0.0],
        [115.2177, -8.6699, 0.0],
        [115.2160, -8.6699, 0.0],
        [115.2160, -8.6710, 0.0]
    ]);
    sqlx::query(
        r#"
        INSERT INTO anims (operasi_id, skenario_id, marker_id, name, anim_type, sort_order, speed, duration_sec, replay, autoplay, geom)
        VALUES ($1, $2, $3, 'Patroli Keliling Sektor 1', 'route', 1, 18.0, 24.0, TRUE, TRUE, $4)
        "#,
    )
    .bind(operasi_id)
    .bind(skenario_id)
    .bind(patwal_id)
    .bind(route_geom)
    .execute(db)
    .await?;

    tracing::info!("Seeded initial demo data (admin/admin123, operator/operator123).");
    Ok(())
}
