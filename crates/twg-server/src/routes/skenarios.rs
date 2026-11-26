use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::Row;
use twg_shared::{
    api::{
        ApiResponse, CopySkenarioRequest, CreateSkenarioRequest, MoveSkenarioRequest,
        PaginatedResponse, UpdateSkenarioRequest,
    },
    models::{BaseLayer, IndoorConfig, MapMode, Skenario, SkenarioBundle, SkyboxPreset},
};
use uuid::Uuid;

use crate::{
    auth::middleware::{AuthUser, verify_skenario_access},
    error::{AppError, AppResult},
    routes::{entities, snapshots},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SkenarioPaginationParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
    pub operasi_id: Option<Uuid>,
}

pub async fn list_skenarios(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<SkenarioPaginationParams>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<Skenario>>>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(10).clamp(1, 200);
    let offset = (page - 1) * per_page;
    let search_pattern = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    if let Some(op_id) = params.operasi_id {
        let op_row = sqlx::query("SELECT satker_id FROM operasis WHERE id = $1")
            .bind(op_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("operasi not found".into()))?;
        let op_satker: Option<Uuid> = op_row.try_get("satker_id")?;
        if !auth.can_access_satker(op_satker) {
            return Err(AppError::Forbidden("cross-satker access denied".into()));
        }
    }

    let (rows, total) = if auth.role.is_satker_role() {
        let rows = sqlx::query(
            r#"
            SELECT sk.id, sk.operasi_id, o.name AS operasi_name, sk.satker_id, sk.name,
                   sk.center_lng, sk.center_lat, sk.zoom, sk.min_zoom, sk.max_zoom,
                   sk.pitch, sk.bearing, sk.map_mode, sk.base_layer, sk.skybox, sk.indoor_config,
                   sk.created_at, sk.updated_at,
                   (SELECT COUNT(*) FROM markers m WHERE m.skenario_id = sk.id) AS marker_count,
                   (SELECT COUNT(*) FROM buildings b WHERE b.skenario_id = sk.id) AS building_count,
                   (SELECT COUNT(*) FROM plot_lines l WHERE l.skenario_id = sk.id) AS line_count,
                   (SELECT COUNT(*) FROM text_markers t WHERE t.skenario_id = sk.id) AS text_count,
                   (SELECT COUNT(*) FROM particles p WHERE p.skenario_id = sk.id) AS particle_count,
                   (SELECT COUNT(*) FROM alurs al WHERE al.skenario_id = sk.id) AS alur_count,
                   (SELECT COUNT(*) FROM anims a WHERE a.skenario_id = sk.id) AS anim_count
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE o.satker_id = $1
              AND ($2::uuid IS NULL OR sk.operasi_id = $2)
              AND ($3::text IS NULL OR sk.name ILIKE $3 OR o.name ILIKE $3 OR sk.map_mode ILIKE $3 OR sk.base_layer ILIKE $3)
            ORDER BY sk.created_at DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(auth.satker_id)
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE o.satker_id = $1
              AND ($2::uuid IS NULL OR sk.operasi_id = $2)
              AND ($3::text IS NULL OR sk.name ILIKE $3 OR o.name ILIKE $3 OR sk.map_mode ILIKE $3 OR sk.base_layer ILIKE $3)
            "#,
        )
        .bind(auth.satker_id)
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    } else if auth.role.as_str() == "user" {
        let rows = sqlx::query(
            r#"
            SELECT sk.id, sk.operasi_id, o.name AS operasi_name, sk.satker_id, sk.name,
                   sk.center_lng, sk.center_lat, sk.zoom, sk.min_zoom, sk.max_zoom,
                   sk.pitch, sk.bearing, sk.map_mode, sk.base_layer, sk.skybox, sk.indoor_config,
                   sk.created_at, sk.updated_at,
                   (SELECT COUNT(*) FROM markers m WHERE m.skenario_id = sk.id) AS marker_count,
                   (SELECT COUNT(*) FROM buildings b WHERE b.skenario_id = sk.id) AS building_count,
                   (SELECT COUNT(*) FROM plot_lines l WHERE l.skenario_id = sk.id) AS line_count,
                   (SELECT COUNT(*) FROM text_markers t WHERE t.skenario_id = sk.id) AS text_count,
                   (SELECT COUNT(*) FROM particles p WHERE p.skenario_id = sk.id) AS particle_count,
                   (SELECT COUNT(*) FROM alurs al WHERE al.skenario_id = sk.id) AS alur_count,
                   (SELECT COUNT(*) FROM anims a WHERE a.skenario_id = sk.id) AS anim_count
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE o.satker_id IS NULL
              AND ($1::uuid IS NULL OR sk.operasi_id = $1)
              AND ($2::text IS NULL OR sk.name ILIKE $2 OR o.name ILIKE $2 OR sk.map_mode ILIKE $2 OR sk.base_layer ILIKE $2)
            ORDER BY sk.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE o.satker_id IS NULL
              AND ($1::uuid IS NULL OR sk.operasi_id = $1)
              AND ($2::text IS NULL OR sk.name ILIKE $2 OR o.name ILIKE $2 OR sk.map_mode ILIKE $2 OR sk.base_layer ILIKE $2)
            "#,
        )
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    } else {
        let rows = sqlx::query(
            r#"
            SELECT sk.id, sk.operasi_id, o.name AS operasi_name, sk.satker_id, sk.name,
                   sk.center_lng, sk.center_lat, sk.zoom, sk.min_zoom, sk.max_zoom,
                   sk.pitch, sk.bearing, sk.map_mode, sk.base_layer, sk.skybox, sk.indoor_config,
                   sk.created_at, sk.updated_at,
                   (SELECT COUNT(*) FROM markers m WHERE m.skenario_id = sk.id) AS marker_count,
                   (SELECT COUNT(*) FROM buildings b WHERE b.skenario_id = sk.id) AS building_count,
                   (SELECT COUNT(*) FROM plot_lines l WHERE l.skenario_id = sk.id) AS line_count,
                   (SELECT COUNT(*) FROM text_markers t WHERE t.skenario_id = sk.id) AS text_count,
                   (SELECT COUNT(*) FROM particles p WHERE p.skenario_id = sk.id) AS particle_count,
                   (SELECT COUNT(*) FROM alurs al WHERE al.skenario_id = sk.id) AS alur_count,
                   (SELECT COUNT(*) FROM anims a WHERE a.skenario_id = sk.id) AS anim_count
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE ($1::uuid IS NULL OR sk.operasi_id = $1)
              AND ($2::text IS NULL OR sk.name ILIKE $2 OR o.name ILIKE $2 OR sk.map_mode ILIKE $2 OR sk.base_layer ILIKE $2)
            ORDER BY sk.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM skenarios sk
            JOIN operasis o ON o.id = sk.operasi_id
            WHERE ($1::uuid IS NULL OR sk.operasi_id = $1)
              AND ($2::text IS NULL OR sk.name ILIKE $2 OR o.name ILIKE $2 OR sk.map_mode ILIKE $2 OR sk.base_layer ILIKE $2)
            "#,
        )
        .bind(params.operasi_id)
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    };

    let items = rows
        .iter()
        .map(map_skenario_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(ApiResponse {
        data: PaginatedResponse {
            items,
            total,
            page,
            per_page,
        },
    }))
}

pub async fn list_skenarios_by_operasi(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(operasi_id): Path<Uuid>,
    Query(mut params): Query<SkenarioPaginationParams>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<Skenario>>>> {
    params.operasi_id = Some(operasi_id);
    list_skenarios(State(state), auth, Query(params)).await
}

pub async fn get_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Skenario>>> {
    verify_skenario_access(&state, &auth, id).await?;
    let sk = fetch_skenario(&state, id).await?;
    Ok(Json(ApiResponse { data: sk }))
}

pub async fn get_skenario_bundle(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<SkenarioBundle>>> {
    verify_skenario_access(&state, &auth, id).await?;

    let skenario = fetch_skenario(&state, id).await?;
    let markers = entities::fetch_markers(&state.db, id).await?;
    let buildings = entities::fetch_buildings(&state.db, id).await?;
    let lines = entities::fetch_lines(&state.db, id).await?;
    let text_markers = entities::fetch_text_markers(&state.db, id).await?;
    let particles = entities::fetch_particles(&state.db, id).await?;
    let alurs = entities::fetch_alurs(&state.db, id).await?;
    let anims = entities::fetch_anims(&state.db, id).await?;
    let snapshot = snapshots::fetch_snapshot_optional(&state.db, id).await?;

    Ok(Json(ApiResponse {
        data: SkenarioBundle {
            skenario,
            markers,
            buildings,
            lines,
            text_markers,
            particles,
            alurs,
            anims,
            snapshot,
        },
    }))
}

pub async fn create_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateSkenarioRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Skenario>>)> {
    auth.require_admin_write()?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("skenario name is required".into()));
    }

    let op_row = sqlx::query("SELECT satker_id FROM operasis WHERE id = $1")
        .bind(req.operasi_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("operasi not found".into()))?;
    let satker_id: Option<Uuid> = op_row.try_get("satker_id")?;
    if !auth.can_mutate_satker(satker_id) {
        return Err(AppError::Forbidden("cross-satker create denied".into()));
    }

    let indoor_json = serde_json::to_value(&req.indoor_config).unwrap_or_default();
    let row = sqlx::query(
        r#"
        INSERT INTO skenarios (
            operasi_id, satker_id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
            pitch, bearing, map_mode, base_layer, skybox, indoor_config
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING id, operasi_id, satker_id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
                  pitch, bearing, map_mode, base_layer, skybox, indoor_config, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(satker_id)
    .bind(req.name.trim())
    .bind(req.center_lng)
    .bind(req.center_lat)
    .bind(req.zoom)
    .bind(req.min_zoom)
    .bind(req.max_zoom)
    .bind(req.pitch)
    .bind(req.bearing)
    .bind(req.map_mode.as_str())
    .bind(req.base_layer.as_str())
    .bind(req.skybox.as_str())
    .bind(indoor_json)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_skenario_row(&row)?,
        }),
    ))
}

pub async fn update_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSkenarioRequest>,
) -> AppResult<Json<ApiResponse<Skenario>>> {
    auth.require_admin_write()?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("skenario name is required".into()));
    }
    let existing = fetch_skenario(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker update denied".into()));
    }

    let indoor_json = serde_json::to_value(&req.indoor_config).unwrap_or_default();
    sqlx::query(
        r#"
        UPDATE skenarios
        SET name = $1, center_lng = $2, center_lat = $3, zoom = $4, min_zoom = $5, max_zoom = $6,
            pitch = $7, bearing = $8, map_mode = $9, base_layer = $10, skybox = $11, indoor_config = $12,
            updated_at = NOW()
        WHERE id = $13
        "#,
    )
    .bind(req.name.trim())
    .bind(req.center_lng)
    .bind(req.center_lat)
    .bind(req.zoom)
    .bind(req.min_zoom)
    .bind(req.max_zoom)
    .bind(req.pitch)
    .bind(req.bearing)
    .bind(req.map_mode.as_str())
    .bind(req.base_layer.as_str())
    .bind(req.skybox.as_str())
    .bind(indoor_json)
    .bind(id)
    .execute(&state.db)
    .await?;

    let sk = fetch_skenario(&state, id).await?;
    Ok(Json(ApiResponse { data: sk }))
}

pub async fn copy_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CopySkenarioRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Skenario>>)> {
    auth.require_admin_write()?;

    let source = fetch_skenario(&state, id).await?;
    if !auth.can_mutate_satker(source.satker_id) {
        return Err(AppError::Forbidden(
            "Tidak memiliki izin menyalin skenario dari satker ini".into(),
        ));
    }

    let target_op_row = sqlx::query("SELECT satker_id FROM operasis WHERE id = $1")
        .bind(req.target_operasi_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Operasi tujuan tidak ditemukan".into()))?;
    let target_satker_id: Option<Uuid> = target_op_row.try_get("satker_id")?;
    if !auth.can_mutate_satker(target_satker_id) {
        return Err(AppError::Forbidden(
            "Tidak memiliki izin menyalin skenario ke operasi satker lain".into(),
        ));
    }

    let new_name = req
        .new_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} (Salinan)", source.name));

    let mut tx = state.db.begin().await?;

    let new_skenario_id = Uuid::new_v4();
    let indoor_json = serde_json::to_value(&source.indoor_config).unwrap_or_default();
    sqlx::query(
        r#"
        INSERT INTO skenarios (
            id, operasi_id, satker_id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
            pitch, bearing, map_mode, base_layer, skybox, indoor_config
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(new_skenario_id)
    .bind(req.target_operasi_id)
    .bind(target_satker_id)
    .bind(&new_name)
    .bind(source.center_lng)
    .bind(source.center_lat)
    .bind(source.zoom)
    .bind(source.min_zoom)
    .bind(source.max_zoom)
    .bind(source.pitch)
    .bind(source.bearing)
    .bind(source.map_mode.as_str())
    .bind(source.base_layer.as_str())
    .bind(source.skybox.as_str())
    .bind(indoor_json)
    .execute(&mut *tx)
    .await?;

    // Fetch all simulation entity rows from source skenario
    let marker_rows = sqlx::query(
        r#"
        SELECT id, unit_id, name, category, model_path, quantity, lng, lat, altitude,
               rot_x, rot_y, rot_z, scale_x, scale_y, scale_z, is_spawn, description, metadata
        FROM markers WHERE skenario_id = $1 ORDER BY created_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let building_rows = sqlx::query(
        "SELECT id, name, description, color, height, geom FROM buildings WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let line_rows = sqlx::query(
        "SELECT id, name, color, width, alpha, geom FROM plot_lines WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let text_rows = sqlx::query(
        "SELECT id, text, color, font_size, show_background, lng, lat, altitude FROM text_markers WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let particle_rows = sqlx::query(
        "SELECT id, name, particle_type, lng, lat, is_spawn, data FROM particles WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let alur_rows = sqlx::query(
        "SELECT id, title, payload FROM alurs WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    let anim_rows = sqlx::query(
        r#"
        SELECT id, marker_id, name, anim_type, sort_order, speed, duration_sec, replay, autoplay, geom, data
        FROM anims WHERE skenario_id = $1 ORDER BY sort_order ASC, created_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    // Pre-allocate all new UUIDs before inserting so cross-references inside JSON blobs
    // (formation member marker_ids, spawn_hide marker_ids, group combine/sequence child anim_ids, etc.)
    // are remapped cleanly in a single pass regardless of row order.
    let mut uuid_remap: HashMap<String, String> = HashMap::new();
    uuid_remap.insert(id.to_string(), new_skenario_id.to_string());
    if source.operasi_id != req.target_operasi_id {
        uuid_remap.insert(
            source.operasi_id.to_string(),
            req.target_operasi_id.to_string(),
        );
    }

    let mut marker_id_map: HashMap<Uuid, Uuid> = HashMap::with_capacity(marker_rows.len());
    for r in &marker_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        marker_id_map.insert(old_id, new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut building_ids = Vec::with_capacity(building_rows.len());
    for r in &building_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        building_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut line_ids = Vec::with_capacity(line_rows.len());
    for r in &line_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        line_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut text_ids = Vec::with_capacity(text_rows.len());
    for r in &text_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        text_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut particle_ids = Vec::with_capacity(particle_rows.len());
    for r in &particle_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        particle_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut alur_ids = Vec::with_capacity(alur_rows.len());
    for r in &alur_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        alur_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    let mut anim_ids = Vec::with_capacity(anim_rows.len());
    for r in &anim_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = Uuid::new_v4();
        anim_ids.push(new_id);
        uuid_remap.insert(old_id.to_string(), new_id.to_string());
    }

    // 1. Insert cloned markers (3D models + basic meshes)
    for r in &marker_rows {
        let old_id: Uuid = r.try_get("id")?;
        let new_id = marker_id_map[&old_id];
        let meta: serde_json::Value = r.try_get("metadata")?;
        let remapped_meta = remap_json_uuids(&meta, &uuid_remap);
        sqlx::query(
            r#"
            INSERT INTO markers (
                id, operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
                lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z,
                is_spawn, description, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("unit_id")?)
        .bind(r.try_get::<String, _>("name")?)
        .bind(r.try_get::<String, _>("category")?)
        .bind(r.try_get::<String, _>("model_path")?)
        .bind(r.try_get::<i32, _>("quantity")?)
        .bind(r.try_get::<f64, _>("lng")?)
        .bind(r.try_get::<f64, _>("lat")?)
        .bind(r.try_get::<f64, _>("altitude")?)
        .bind(r.try_get::<f64, _>("rot_x")?)
        .bind(r.try_get::<f64, _>("rot_y")?)
        .bind(r.try_get::<f64, _>("rot_z")?)
        .bind(r.try_get::<f64, _>("scale_x")?)
        .bind(r.try_get::<f64, _>("scale_y")?)
        .bind(r.try_get::<f64, _>("scale_z")?)
        .bind(r.try_get::<bool, _>("is_spawn")?)
        .bind(r.try_get::<Option<String>, _>("description")?)
        .bind(remapped_meta)
        .execute(&mut *tx)
        .await?;
    }

    // 2. Insert cloned buildings / extruded polygons
    for (r, new_id) in building_rows.iter().zip(building_ids) {
        sqlx::query(
            r#"
            INSERT INTO buildings (id, operasi_id, skenario_id, name, description, color, height, geom)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("name")?)
        .bind(r.try_get::<Option<String>, _>("description")?)
        .bind(r.try_get::<String, _>("color")?)
        .bind(r.try_get::<f64, _>("height")?)
        .bind(r.try_get::<serde_json::Value, _>("geom")?)
        .execute(&mut *tx)
        .await?;
    }

    // 3. Insert cloned plot_lines
    for (r, new_id) in line_rows.iter().zip(line_ids) {
        sqlx::query(
            r#"
            INSERT INTO plot_lines (id, operasi_id, skenario_id, name, color, width, alpha, geom)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("name")?)
        .bind(r.try_get::<String, _>("color")?)
        .bind(r.try_get::<f64, _>("width")?)
        .bind(r.try_get::<f64, _>("alpha")?)
        .bind(r.try_get::<serde_json::Value, _>("geom")?)
        .execute(&mut *tx)
        .await?;
    }

    // 4. Insert cloned 3D text_markers
    for (r, new_id) in text_rows.iter().zip(text_ids) {
        sqlx::query(
            r#"
            INSERT INTO text_markers (id, operasi_id, skenario_id, text, color, font_size, show_background, lng, lat, altitude)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("text")?)
        .bind(r.try_get::<String, _>("color")?)
        .bind(r.try_get::<f64, _>("font_size")?)
        .bind(r.try_get::<bool, _>("show_background")?)
        .bind(r.try_get::<f64, _>("lng")?)
        .bind(r.try_get::<f64, _>("lat")?)
        .bind(r.try_get::<f64, _>("altitude")?)
        .execute(&mut *tx)
        .await?;
    }

    // 5. Insert cloned particles
    for (r, new_id) in particle_rows.iter().zip(particle_ids) {
        let data: serde_json::Value = r.try_get("data")?;
        let remapped_data = remap_json_uuids(&data, &uuid_remap);
        sqlx::query(
            r#"
            INSERT INTO particles (id, operasi_id, skenario_id, name, particle_type, lng, lat, is_spawn, data)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("name")?)
        .bind(r.try_get::<String, _>("particle_type")?)
        .bind(r.try_get::<f64, _>("lng")?)
        .bind(r.try_get::<f64, _>("lat")?)
        .bind(r.try_get::<bool, _>("is_spawn")?)
        .bind(remapped_data)
        .execute(&mut *tx)
        .await?;
    }

    // 6. Insert cloned alurs
    for (r, new_id) in alur_rows.iter().zip(alur_ids) {
        let payload: serde_json::Value = r.try_get("payload")?;
        let remapped_payload = remap_json_uuids(&payload, &uuid_remap);
        sqlx::query(
            r#"
            INSERT INTO alurs (id, operasi_id, skenario_id, title, payload)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(r.try_get::<String, _>("title")?)
        .bind(remapped_payload)
        .execute(&mut *tx)
        .await?;
    }

    // 7. Insert cloned anims with remapped marker_id and remapped JSON config (formations, spawn/hide, group combine/sequence, shoot, pointer, particle)
    for (r, new_id) in anim_rows.iter().zip(anim_ids) {
        let old_marker_id: Option<Uuid> = r.try_get("marker_id")?;
        let new_marker_id = old_marker_id.and_then(|mid| marker_id_map.get(&mid).copied());
        let data: serde_json::Value = r.try_get("data")?;
        let remapped_data = remap_json_uuids(&data, &uuid_remap);
        sqlx::query(
            r#"
            INSERT INTO anims (
                id, operasi_id, skenario_id, marker_id, name, anim_type, sort_order,
                speed, duration_sec, replay, autoplay, geom, data
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(new_id)
        .bind(req.target_operasi_id)
        .bind(new_skenario_id)
        .bind(new_marker_id)
        .bind(r.try_get::<String, _>("name")?)
        .bind(r.try_get::<String, _>("anim_type")?)
        .bind(r.try_get::<i32, _>("sort_order")?)
        .bind(r.try_get::<f64, _>("speed")?)
        .bind(r.try_get::<f64, _>("duration_sec")?)
        .bind(r.try_get::<bool, _>("replay")?)
        .bind(r.try_get::<bool, _>("autoplay")?)
        .bind(r.try_get::<serde_json::Value, _>("geom")?)
        .bind(remapped_data)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let created = fetch_skenario(&state, new_skenario_id).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse { data: created })))
}

pub async fn move_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<MoveSkenarioRequest>,
) -> AppResult<Json<ApiResponse<Skenario>>> {
    auth.require_admin_write()?;

    let existing = fetch_skenario(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden(
            "Tidak memiliki izin memindahkan skenario dari satker ini".into(),
        ));
    }
    if req.target_operasi_id == existing.operasi_id {
        return Err(AppError::BadRequest(
            "Skenario sudah berada di operasi tujuan tersebut.".into(),
        ));
    }

    let target_op_row = sqlx::query("SELECT satker_id FROM operasis WHERE id = $1")
        .bind(req.target_operasi_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Operasi tujuan tidak ditemukan".into()))?;
    let target_satker_id: Option<Uuid> = target_op_row.try_get("satker_id")?;
    if !auth.can_mutate_satker(target_satker_id) {
        return Err(AppError::Forbidden(
            "Tidak memiliki izin memindahkan skenario ke operasi satker lain".into(),
        ));
    }

    let mut tx = state.db.begin().await?;

    sqlx::query(
        "UPDATE skenarios SET operasi_id = $1, satker_id = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(req.target_operasi_id)
    .bind(target_satker_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    for sql in [
        "UPDATE markers SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE buildings SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE plot_lines SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE text_markers SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE particles SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE alurs SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE anims SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE game_session_snapshots SET operasi_id = $1, updated_at = NOW() WHERE skenario_id = $2",
        "UPDATE commands SET operasi_id = $1 WHERE skenario_id = $2",
    ] {
        sqlx::query(sql)
            .bind(req.target_operasi_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    let moved = fetch_skenario(&state, id).await?;
    Ok(Json(ApiResponse { data: moved }))
}

pub async fn delete_skenario(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_admin_write()?;
    let existing = fetch_skenario(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker delete denied".into()));
    }

    if existing.has_simulation_content() {
        let detail = existing.content_summary();
        return Err(AppError::BadRequest(format!(
            "Skenario tidak dapat dihapus karena masih memiliki isi simulasi ({detail}). Kosongkan seluruh isi di Plot Skenario terlebih dahulu."
        )));
    }

    sqlx::query("DELETE FROM skenarios WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn remap_json_uuids(val: &serde_json::Value, remap: &HashMap<String, String>) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => {
            if let Some(new_s) = remap.get(s) {
                serde_json::Value::String(new_s.clone())
            } else {
                let lower = s.to_ascii_lowercase();
                if let Some(new_s) = remap.get(&lower) {
                    serde_json::Value::String(new_s.clone())
                } else {
                    serde_json::Value::String(s.clone())
                }
            }
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| remap_json_uuids(v, remap)).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let new_k = remap
                    .get(k)
                    .or_else(|| remap.get(&k.to_ascii_lowercase()))
                    .cloned()
                    .unwrap_or_else(|| k.clone());
                out.insert(new_k, remap_json_uuids(v, remap));
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

pub async fn fetch_skenario(state: &AppState, id: Uuid) -> AppResult<Skenario> {
    let row = sqlx::query(
        r#"
        SELECT sk.id, sk.operasi_id, o.name AS operasi_name, sk.satker_id, sk.name,
               sk.center_lng, sk.center_lat, sk.zoom, sk.min_zoom, sk.max_zoom,
               sk.pitch, sk.bearing, sk.map_mode, sk.base_layer, sk.skybox, sk.indoor_config,
               sk.created_at, sk.updated_at,
               (SELECT COUNT(*) FROM markers m WHERE m.skenario_id = sk.id) AS marker_count,
               (SELECT COUNT(*) FROM buildings b WHERE b.skenario_id = sk.id) AS building_count,
               (SELECT COUNT(*) FROM plot_lines l WHERE l.skenario_id = sk.id) AS line_count,
               (SELECT COUNT(*) FROM text_markers t WHERE t.skenario_id = sk.id) AS text_count,
               (SELECT COUNT(*) FROM particles p WHERE p.skenario_id = sk.id) AS particle_count,
               (SELECT COUNT(*) FROM alurs al WHERE al.skenario_id = sk.id) AS alur_count,
               (SELECT COUNT(*) FROM anims a WHERE a.skenario_id = sk.id) AS anim_count
        FROM skenarios sk
        JOIN operasis o ON o.id = sk.operasi_id
        WHERE sk.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("skenario not found".into()))?;

    Ok(map_skenario_row(&row)?)
}

pub(crate) fn map_skenario_row(row: &sqlx::postgres::PgRow) -> Result<Skenario, sqlx::Error> {
    let map_mode_str: String = row.try_get("map_mode")?;
    let base_layer_str: String = row.try_get("base_layer")?;
    let skybox_str: String = row
        .try_get("skybox")
        .unwrap_or_else(|_| "day_clear".to_string());
    let indoor_val: serde_json::Value = row.try_get("indoor_config")?;
    let indoor_config: IndoorConfig = serde_json::from_value(indoor_val).unwrap_or_default();

    Ok(Skenario {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        operasi_name: row.try_get("operasi_name").ok().flatten(),
        satker_id: row.try_get("satker_id")?,
        name: row.try_get("name")?,
        center_lng: row.try_get("center_lng")?,
        center_lat: row.try_get("center_lat")?,
        zoom: row.try_get("zoom")?,
        min_zoom: row.try_get("min_zoom")?,
        max_zoom: row.try_get("max_zoom")?,
        pitch: row.try_get("pitch")?,
        bearing: row.try_get("bearing")?,
        map_mode: MapMode::from_db(&map_mode_str),
        base_layer: BaseLayer::from_db(&base_layer_str),
        skybox: SkyboxPreset::from_db(&skybox_str),
        indoor_config,
        marker_count: row.try_get("marker_count").unwrap_or(0),
        building_count: row.try_get("building_count").unwrap_or(0),
        line_count: row.try_get("line_count").unwrap_or(0),
        text_count: row.try_get("text_count").unwrap_or(0),
        particle_count: row.try_get("particle_count").unwrap_or(0),
        alur_count: row.try_get("alur_count").unwrap_or(0),
        anim_count: row.try_get("anim_count").unwrap_or(0),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_json_uuids_remaps_nested_values_arrays_and_object_keys() {
        let old_m1 = Uuid::new_v4();
        let new_m1 = Uuid::new_v4();
        let old_m2 = Uuid::new_v4();
        let new_m2 = Uuid::new_v4();
        let old_anim = Uuid::new_v4();
        let new_anim = Uuid::new_v4();
        let untouched = Uuid::new_v4();

        let mut remap = HashMap::new();
        remap.insert(old_m1.to_string(), new_m1.to_string());
        remap.insert(old_m2.to_string(), new_m2.to_string());
        remap.insert(old_anim.to_string(), new_anim.to_string());

        let original = serde_json::json!({
            "formation": {
                "extra_marker_ids": [old_m1.to_string(), old_m2.to_string().to_uppercase()],
                "offsets_by_marker": {
                    old_m1.to_string(): [2.0, 0.0],
                    untouched.to_string(): [1.0, 1.0]
                }
            },
            "group_config": {
                "mode": "sequence",
                "steps": [
                    { "anim_id": old_anim.to_string(), "delay_sec": 0.25 }
                ]
            }
        });

        let remapped = remap_json_uuids(&original, &remap);
        assert_eq!(
            remapped["formation"]["extra_marker_ids"][0].as_str(),
            Some(new_m1.to_string().as_str())
        );
        assert_eq!(
            remapped["formation"]["extra_marker_ids"][1].as_str(),
            Some(new_m2.to_string().as_str())
        );
        assert!(
            remapped["formation"]["offsets_by_marker"]
                .get(new_m1.to_string())
                .is_some()
        );
        assert!(
            remapped["formation"]["offsets_by_marker"]
                .get(old_m1.to_string())
                .is_none()
        );
        assert!(
            remapped["formation"]["offsets_by_marker"]
                .get(untouched.to_string())
                .is_some()
        );
        assert_eq!(
            remapped["group_config"]["steps"][0]["anim_id"].as_str(),
            Some(new_anim.to_string().as_str())
        );
    }
}
