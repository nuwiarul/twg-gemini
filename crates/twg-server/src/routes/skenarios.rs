use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::Row;
use twg_shared::{
    api::{ApiResponse, CreateSkenarioRequest, PaginatedResponse, UpdateSkenarioRequest},
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
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("skenario name is required".into()));
    }
    let existing = fetch_skenario(&state, id).await?;
    if !auth.can_access_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker access denied".into()));
    }
    if auth.role.is_admin_monitor() {
        return Err(AppError::Forbidden("monitor role is read-only".into()));
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
    sqlx::query("DELETE FROM skenarios WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
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

fn map_skenario_row(row: &sqlx::postgres::PgRow) -> Result<Skenario, sqlx::Error> {
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
        anim_count: row.try_get("anim_count").unwrap_or(0),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
