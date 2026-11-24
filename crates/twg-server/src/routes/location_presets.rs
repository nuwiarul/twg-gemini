use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sqlx::Row;
use twg_shared::{
    api::{ApiResponse, CreateLocationPresetRequest, UpdateLocationPresetRequest},
    models::{BaseLayer, LocationPreset, MapMode},
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    state::AppState,
};

pub async fn list_location_presets(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> AppResult<Json<ApiResponse<Vec<LocationPreset>>>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
               pitch, bearing, map_mode, base_layer, grid_size_m, sort_order,
               created_at, updated_at
        FROM location_presets
        ORDER BY sort_order ASC, name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let items = rows
        .iter()
        .map(map_preset_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(ApiResponse { data: items }))
}

pub async fn create_location_preset(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateLocationPresetRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<LocationPreset>>)> {
    auth.require_admin_write()?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("preset name is required".into()));
    }
    let min_zoom = req.min_zoom.clamp(1.0, 26.0);
    let max_zoom = req.max_zoom.clamp(min_zoom, 26.0);
    let zoom = req.zoom.clamp(min_zoom, max_zoom);
    let pitch = req.pitch.clamp(5.0, 82.0);
    let bearing = req.bearing.rem_euclid(360.0);
    let grid_size_m = req.grid_size_m.clamp(1.0, 200.0);

    let row = sqlx::query(
        r#"
        INSERT INTO location_presets (
            name, center_lng, center_lat, zoom, min_zoom, max_zoom,
            pitch, bearing, map_mode, base_layer, grid_size_m, sort_order
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
                  pitch, bearing, map_mode, base_layer, grid_size_m, sort_order,
                  created_at, updated_at
        "#,
    )
    .bind(name)
    .bind(req.center_lng.clamp(-180.0, 180.0))
    .bind(req.center_lat.clamp(-85.0, 85.0))
    .bind(zoom)
    .bind(min_zoom)
    .bind(max_zoom)
    .bind(pitch)
    .bind(bearing)
    .bind(req.map_mode.as_str())
    .bind(req.base_layer.as_str())
    .bind(grid_size_m)
    .bind(req.sort_order)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_preset_row(&row)?,
        }),
    ))
}

pub async fn update_location_preset(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateLocationPresetRequest>,
) -> AppResult<Json<ApiResponse<LocationPreset>>> {
    auth.require_admin_write()?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("preset name is required".into()));
    }
    let min_zoom = req.min_zoom.clamp(1.0, 26.0);
    let max_zoom = req.max_zoom.clamp(min_zoom, 26.0);
    let zoom = req.zoom.clamp(min_zoom, max_zoom);
    let pitch = req.pitch.clamp(5.0, 82.0);
    let bearing = req.bearing.rem_euclid(360.0);
    let grid_size_m = req.grid_size_m.clamp(1.0, 200.0);

    let row = sqlx::query(
        r#"
        UPDATE location_presets
        SET name = $1, center_lng = $2, center_lat = $3, zoom = $4,
            min_zoom = $5, max_zoom = $6, pitch = $7, bearing = $8,
            map_mode = $9, base_layer = $10, grid_size_m = $11, sort_order = $12,
            updated_at = NOW()
        WHERE id = $13
        RETURNING id, name, center_lng, center_lat, zoom, min_zoom, max_zoom,
                  pitch, bearing, map_mode, base_layer, grid_size_m, sort_order,
                  created_at, updated_at
        "#,
    )
    .bind(name)
    .bind(req.center_lng.clamp(-180.0, 180.0))
    .bind(req.center_lat.clamp(-85.0, 85.0))
    .bind(zoom)
    .bind(min_zoom)
    .bind(max_zoom)
    .bind(pitch)
    .bind(bearing)
    .bind(req.map_mode.as_str())
    .bind(req.base_layer.as_str())
    .bind(grid_size_m)
    .bind(req.sort_order)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("location preset not found".into()))?;

    Ok(Json(ApiResponse {
        data: map_preset_row(&row)?,
    }))
}

pub async fn delete_location_preset(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_admin_write()?;
    let res = sqlx::query("DELETE FROM location_presets WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("location preset not found".into()));
    }
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_preset_row(row: &sqlx::postgres::PgRow) -> Result<LocationPreset, sqlx::Error> {
    let map_mode_str: String = row.try_get("map_mode")?;
    let base_layer_str: String = row.try_get("base_layer")?;
    Ok(LocationPreset {
        id: row.try_get("id")?,
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
        grid_size_m: row.try_get("grid_size_m")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
