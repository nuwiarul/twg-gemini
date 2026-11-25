use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sqlx::{PgPool, Row};
use twg_shared::{
    api::{
        ApiResponse, CreateAlurRequest, CreateAnimRequest, CreateBuildingRequest,
        CreateMarkerRequest, CreateParticleRequest, CreatePlotLineRequest,
        CreateTextMarkerRequest, UpdateAnimRequest, UpdateBuildingRequest, UpdateMarkerRequest,
        UpdatePlotLineRequest, UpdateTextMarkerRequest,
    },
    models::{Alur, Anim, Building, Marker, Particle, PlotLine, TextMarker},
};
use uuid::Uuid;

use crate::{
    auth::middleware::{AuthUser, verify_skenario_access},
    error::{AppError, AppResult},
    state::AppState,
};

fn ensure_can_edit(auth: &AuthUser) -> AppResult<()> {
    if auth.role.is_big_screen_only_role() {
        return Err(AppError::Forbidden(
            "Akun Layar Besar bersifat read-only dan tidak dapat mengedit skenario".into(),
        ));
    }
    Ok(())
}

// =============================================================================
// Markers
// =============================================================================

pub async fn fetch_markers(db: &PgPool, skenario_id: Uuid) -> Result<Vec<Marker>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
               lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z,
               is_spawn, description, metadata, created_at, updated_at
        FROM markers
        WHERE skenario_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_marker_row).collect()
}

pub async fn create_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateMarkerRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Marker>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;
    let meta = if req.metadata.is_null() {
        serde_json::json!({})
    } else {
        req.metadata
    };

    let row = sqlx::query(
        r#"
        INSERT INTO markers (
            operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
            lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z,
            is_spawn, description, metadata
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
        RETURNING id, operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
                  lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z,
                  is_spawn, description, metadata, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.unit_id)
    .bind(req.name.trim())
    .bind(req.category)
    .bind(req.model_path)
    .bind(req.quantity)
    .bind(req.lng)
    .bind(req.lat)
    .bind(req.altitude)
    .bind(req.rot_x)
    .bind(req.rot_y)
    .bind(req.rot_z)
    .bind(req.scale_x)
    .bind(req.scale_y)
    .bind(req.scale_z)
    .bind(req.is_spawn)
    .bind(req.description)
    .bind(meta)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_marker_row(&row)?,
        }),
    ))
}

pub async fn update_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMarkerRequest>,
) -> AppResult<Json<ApiResponse<Marker>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM markers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("marker not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let meta = if req.metadata.is_null() {
        serde_json::json!({})
    } else {
        req.metadata
    };

    let row = sqlx::query(
        r#"
        UPDATE markers
        SET name = $1, lng = $2, lat = $3, altitude = $4, rot_x = $5, rot_y = $6, rot_z = $7,
            scale_x = $8, scale_y = $9, scale_z = $10, is_spawn = $11, description = $12,
            metadata = $13, updated_at = NOW()
        WHERE id = $14
        RETURNING id, operasi_id, skenario_id, unit_id, name, category, model_path, quantity,
                  lng, lat, altitude, rot_x, rot_y, rot_z, scale_x, scale_y, scale_z,
                  is_spawn, description, metadata, created_at, updated_at
        "#,
    )
    .bind(req.name.trim())
    .bind(req.lng)
    .bind(req.lat)
    .bind(req.altitude)
    .bind(req.rot_x)
    .bind(req.rot_y)
    .bind(req.rot_z)
    .bind(req.scale_x)
    .bind(req.scale_y)
    .bind(req.scale_z)
    .bind(req.is_spawn)
    .bind(req.description)
    .bind(meta)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_marker_row(&row)?,
    }))
}

pub async fn delete_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM markers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("marker not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    sqlx::query("DELETE FROM markers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_marker_row(row: &sqlx::postgres::PgRow) -> Result<Marker, sqlx::Error> {
    Ok(Marker {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        unit_id: row.try_get("unit_id")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        model_path: row.try_get("model_path")?,
        quantity: row.try_get("quantity")?,
        lng: row.try_get("lng")?,
        lat: row.try_get("lat")?,
        altitude: row.try_get("altitude")?,
        rot_x: row.try_get("rot_x")?,
        rot_y: row.try_get("rot_y")?,
        rot_z: row.try_get("rot_z")?,
        scale_x: row.try_get("scale_x")?,
        scale_y: row.try_get("scale_y")?,
        scale_z: row.try_get("scale_z")?,
        is_spawn: row.try_get("is_spawn")?,
        description: row.try_get("description")?,
        metadata: row.try_get("metadata")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// =============================================================================
// Buildings
// =============================================================================

pub async fn fetch_buildings(db: &PgPool, skenario_id: Uuid) -> Result<Vec<Building>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, name, description, color, height, geom, created_at, updated_at
        FROM buildings
        WHERE skenario_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_building_row).collect()
}

pub async fn create_building(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateBuildingRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Building>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;
    if req.geom.len() < 3 {
        return Err(AppError::BadRequest(
            "building polygon needs at least 3 vertices".into(),
        ));
    }

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let row = sqlx::query(
        r#"
        INSERT INTO buildings (operasi_id, skenario_id, name, description, color, height, geom)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, operasi_id, skenario_id, name, description, color, height, geom, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.name.trim())
    .bind(req.description)
    .bind(req.color)
    .bind(req.height)
    .bind(geom_json)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_building_row(&row)?,
        }),
    ))
}

pub async fn update_building(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBuildingRequest>,
) -> AppResult<Json<ApiResponse<Building>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM buildings WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("building not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let row = sqlx::query(
        r#"
        UPDATE buildings
        SET name = $1, description = $2, color = $3, height = $4, geom = $5, updated_at = NOW()
        WHERE id = $6
        RETURNING id, operasi_id, skenario_id, name, description, color, height, geom, created_at, updated_at
        "#,
    )
    .bind(req.name.trim())
    .bind(req.description)
    .bind(req.color)
    .bind(req.height)
    .bind(geom_json)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_building_row(&row)?,
    }))
}

pub async fn delete_building(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM buildings WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("building not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM buildings WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_building_row(row: &sqlx::postgres::PgRow) -> Result<Building, sqlx::Error> {
    let geom_val: serde_json::Value = row.try_get("geom")?;
    let geom: Vec<[f64; 2]> = serde_json::from_value(geom_val).unwrap_or_default();
    Ok(Building {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        color: row.try_get("color")?,
        height: row.try_get("height")?,
        geom,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// =============================================================================
// Plot Lines
// =============================================================================

pub async fn fetch_lines(db: &PgPool, skenario_id: Uuid) -> Result<Vec<PlotLine>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, name, color, width, alpha, geom, created_at, updated_at
        FROM plot_lines
        WHERE skenario_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_line_row).collect()
}

pub async fn create_line(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreatePlotLineRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<PlotLine>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;
    if req.geom.len() < 2 {
        return Err(AppError::BadRequest(
            "plot line needs at least 2 points".into(),
        ));
    }

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let row = sqlx::query(
        r#"
        INSERT INTO plot_lines (operasi_id, skenario_id, name, color, width, alpha, geom)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, operasi_id, skenario_id, name, color, width, alpha, geom, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.name.trim())
    .bind(req.color)
    .bind(req.width)
    .bind(req.alpha)
    .bind(geom_json)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_line_row(&row)?,
        }),
    ))
}

pub async fn update_line(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePlotLineRequest>,
) -> AppResult<Json<ApiResponse<PlotLine>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM plot_lines WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("line not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let row = sqlx::query(
        r#"
        UPDATE plot_lines
        SET name = $1, color = $2, width = $3, alpha = $4, geom = $5, updated_at = NOW()
        WHERE id = $6
        RETURNING id, operasi_id, skenario_id, name, color, width, alpha, geom, created_at, updated_at
        "#,
    )
    .bind(req.name.trim())
    .bind(req.color)
    .bind(req.width)
    .bind(req.alpha)
    .bind(geom_json)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_line_row(&row)?,
    }))
}

pub async fn delete_line(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM plot_lines WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("line not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM plot_lines WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_line_row(row: &sqlx::postgres::PgRow) -> Result<PlotLine, sqlx::Error> {
    let geom_val: serde_json::Value = row.try_get("geom")?;
    let geom: Vec<[f64; 2]> = serde_json::from_value(geom_val).unwrap_or_default();
    Ok(PlotLine {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        name: row.try_get("name")?,
        color: row.try_get("color")?,
        width: row.try_get("width")?,
        alpha: row.try_get("alpha")?,
        geom,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// =============================================================================
// Text Markers
// =============================================================================

pub async fn fetch_text_markers(
    db: &PgPool,
    skenario_id: Uuid,
) -> Result<Vec<TextMarker>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, text, color, font_size, lng, lat, altitude, created_at, updated_at
        FROM text_markers
        WHERE skenario_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_text_marker_row).collect()
}

pub async fn create_text_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTextMarkerRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<TextMarker>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO text_markers (operasi_id, skenario_id, text, color, font_size, lng, lat, altitude)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, operasi_id, skenario_id, text, color, font_size, lng, lat, altitude, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.text.trim())
    .bind(req.color)
    .bind(req.font_size)
    .bind(req.lng)
    .bind(req.lat)
    .bind(req.altitude)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_text_marker_row(&row)?,
        }),
    ))
}

pub async fn update_text_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTextMarkerRequest>,
) -> AppResult<Json<ApiResponse<TextMarker>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid =
        sqlx::query_scalar("SELECT skenario_id FROM text_markers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("text marker not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let row = sqlx::query(
        r#"
        UPDATE text_markers
        SET text = $1, color = $2, font_size = $3, lng = $4, lat = $5, altitude = $6, updated_at = NOW()
        WHERE id = $7
        RETURNING id, operasi_id, skenario_id, text, color, font_size, lng, lat, altitude, created_at, updated_at
        "#,
    )
    .bind(req.text.trim())
    .bind(req.color)
    .bind(req.font_size)
    .bind(req.lng)
    .bind(req.lat)
    .bind(req.altitude)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_text_marker_row(&row)?,
    }))
}

pub async fn delete_text_marker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid =
        sqlx::query_scalar("SELECT skenario_id FROM text_markers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("text marker not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM text_markers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_text_marker_row(row: &sqlx::postgres::PgRow) -> Result<TextMarker, sqlx::Error> {
    Ok(TextMarker {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        text: row.try_get("text")?,
        color: row.try_get("color")?,
        font_size: row.try_get("font_size")?,
        lng: row.try_get("lng")?,
        lat: row.try_get("lat")?,
        altitude: row.try_get("altitude")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// =============================================================================
// Particles, Alurs, Anims
// =============================================================================

pub async fn fetch_particles(db: &PgPool, skenario_id: Uuid) -> Result<Vec<Particle>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, name, particle_type, lng, lat, is_spawn, data, created_at, updated_at
        FROM particles
        WHERE skenario_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_particle_row).collect()
}

pub async fn create_particle(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateParticleRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Particle>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;
    let data = if req.data.is_null() {
        serde_json::json!({})
    } else {
        req.data
    };

    let row = sqlx::query(
        r#"
        INSERT INTO particles (operasi_id, skenario_id, name, particle_type, lng, lat, is_spawn, data)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, operasi_id, skenario_id, name, particle_type, lng, lat, is_spawn, data, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.name.trim())
    .bind(req.particle_type)
    .bind(req.lng)
    .bind(req.lat)
    .bind(req.is_spawn)
    .bind(data)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_particle_row(&row)?,
        }),
    ))
}

pub async fn delete_particle(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM particles WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("particle not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM particles WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_particle_row(row: &sqlx::postgres::PgRow) -> Result<Particle, sqlx::Error> {
    Ok(Particle {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        name: row.try_get("name")?,
        particle_type: row.try_get("particle_type")?,
        lng: row.try_get("lng")?,
        lat: row.try_get("lat")?,
        is_spawn: row.try_get("is_spawn")?,
        data: row.try_get("data")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn fetch_alurs(db: &PgPool, skenario_id: Uuid) -> Result<Vec<Alur>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, operasi_id, skenario_id, title, payload, created_at, updated_at FROM alurs WHERE skenario_id = $1 ORDER BY created_at ASC",
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_alur_row).collect()
}

pub async fn create_alur(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateAlurRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Alur>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;
    let payload = if req.payload.is_null() {
        serde_json::json!({})
    } else {
        req.payload
    };

    let row = sqlx::query(
        r#"
        INSERT INTO alurs (operasi_id, skenario_id, title, payload)
        VALUES ($1, $2, $3, $4)
        RETURNING id, operasi_id, skenario_id, title, payload, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.title.trim())
    .bind(payload)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_alur_row(&row)?,
        }),
    ))
}

pub async fn delete_alur(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM alurs WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("alur not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM alurs WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_alur_row(row: &sqlx::postgres::PgRow) -> Result<Alur, sqlx::Error> {
    Ok(Alur {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        title: row.try_get("title")?,
        payload: row.try_get("payload")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn fetch_anims(db: &PgPool, skenario_id: Uuid) -> Result<Vec<Anim>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, marker_id, name, anim_type, sort_order,
               speed, duration_sec, replay, autoplay, geom, data, created_at, updated_at
        FROM anims
        WHERE skenario_id = $1
        ORDER BY sort_order ASC, created_at ASC
        "#,
    )
    .bind(skenario_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(map_anim_row).collect()
}

pub async fn create_anim(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateAnimRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Anim>>)> {
    ensure_can_edit(&auth)?;
    verify_skenario_access(&state, &auth, req.skenario_id).await?;

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let data = if req.data.is_null() {
        serde_json::json!({})
    } else {
        req.data
    };

    let row = sqlx::query(
        r#"
        INSERT INTO anims (
            operasi_id, skenario_id, marker_id, name, anim_type, sort_order,
            speed, duration_sec, replay, autoplay, geom, data
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        RETURNING id, operasi_id, skenario_id, marker_id, name, anim_type, sort_order,
                  speed, duration_sec, replay, autoplay, geom, data, created_at, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(req.skenario_id)
    .bind(req.marker_id)
    .bind(req.name.trim())
    .bind(req.anim_type)
    .bind(req.sort_order)
    .bind(req.speed)
    .bind(req.duration_sec)
    .bind(req.replay)
    .bind(req.autoplay)
    .bind(geom_json)
    .bind(data)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            data: map_anim_row(&row)?,
        }),
    ))
}

pub async fn update_anim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAnimRequest>,
) -> AppResult<Json<ApiResponse<Anim>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM anims WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("anim not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let geom_json = serde_json::to_value(&req.geom).unwrap_or_default();
    let data = if req.data.is_null() {
        serde_json::json!({})
    } else {
        req.data
    };

    let row = sqlx::query(
        r#"
        UPDATE anims
        SET marker_id = $1, name = $2, anim_type = $3, sort_order = $4, speed = $5,
            duration_sec = $6, replay = $7, autoplay = $8, geom = $9, data = $10, updated_at = NOW()
        WHERE id = $11
        RETURNING id, operasi_id, skenario_id, marker_id, name, anim_type, sort_order,
                  speed, duration_sec, replay, autoplay, geom, data, created_at, updated_at
        "#,
    )
    .bind(req.marker_id)
    .bind(req.name.trim())
    .bind(req.anim_type)
    .bind(req.sort_order)
    .bind(req.speed)
    .bind(req.duration_sec)
    .bind(req.replay)
    .bind(req.autoplay)
    .bind(geom_json)
    .bind(data)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_anim_row(&row)?,
    }))
}

pub async fn delete_anim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ensure_can_edit(&auth)?;
    let skenario_id: Uuid = sqlx::query_scalar("SELECT skenario_id FROM anims WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("anim not found".into()))?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM anims WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_anim_row(row: &sqlx::postgres::PgRow) -> Result<Anim, sqlx::Error> {
    let geom_val: serde_json::Value = row.try_get("geom")?;
    let geom: Vec<[f64; 3]> = serde_json::from_value(geom_val).unwrap_or_default();
    Ok(Anim {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        marker_id: row.try_get("marker_id")?,
        name: row.try_get("name")?,
        anim_type: row.try_get("anim_type")?,
        sort_order: row.try_get("sort_order")?,
        speed: row.try_get("speed")?,
        duration_sec: row.try_get("duration_sec")?,
        replay: row.try_get("replay")?,
        autoplay: row.try_get("autoplay")?,
        geom,
        data: row.try_get("data")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
