use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::Row;
use twg_shared::{
    api::{ApiResponse, CreateSatkerRequest, UpdateSatkerRequest},
    models::Satker,
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    state::AppState,
};

pub async fn list_satkers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<ApiResponse<Vec<Satker>>>> {
    if auth.role.as_str() == "user" || auth.role.is_big_screen_only_role() {
        return Err(AppError::Forbidden(
            "Akses daftar satker dibatasi untuk role ini".into(),
        ));
    }

    let rows = if auth.role.is_satker_role() {
        sqlx::query(
            "SELECT id, code, name, is_active, created_by, created_at, updated_at FROM satkers WHERE id = $1 ORDER BY name",
        )
        .bind(auth.satker_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query(
            "SELECT id, code, name, is_active, created_by, created_at, updated_at FROM satkers ORDER BY name",
        )
        .fetch_all(&state.db)
        .await?
    };

    let items = rows
        .iter()
        .map(map_satker_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(ApiResponse { data: items }))
}

pub async fn get_satker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Satker>>> {
    if !auth.can_access_satker(Some(id)) {
        return Err(AppError::Forbidden("cross-satker access denied".into()));
    }

    let row = sqlx::query(
        "SELECT id, code, name, is_active, created_by, created_at, updated_at FROM satkers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("satker not found".into()))?;

    Ok(Json(ApiResponse {
        data: map_satker_row(&row)?,
    }))
}

pub async fn create_satker(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateSatkerRequest>,
) -> AppResult<Json<ApiResponse<Satker>>> {
    auth.require_super_admin_write()?;
    let code = req.code.trim();
    let name = req.name.trim();
    if code.is_empty() || name.is_empty() {
        return Err(AppError::BadRequest(
            "Kode dan nama satker wajib diisi".into(),
        ));
    }
    let dup: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM satkers WHERE LOWER(code) = LOWER($1))")
            .bind(code)
            .fetch_one(&state.db)
            .await?;
    if dup {
        return Err(AppError::BadRequest(format!(
            "Kode satker '{code}' sudah digunakan"
        )));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO satkers (code, name, is_active, created_by)
        VALUES ($1, $2, $3, $4)
        RETURNING id, code, name, is_active, created_by, created_at, updated_at
        "#,
    )
    .bind(req.code.trim())
    .bind(req.name.trim())
    .bind(req.is_active)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_satker_row(&row)?,
    }))
}

pub async fn update_satker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSatkerRequest>,
) -> AppResult<Json<ApiResponse<Satker>>> {
    auth.require_super_admin_write()?;
    let code = req.code.trim();
    let name = req.name.trim();
    if code.is_empty() || name.is_empty() {
        return Err(AppError::BadRequest(
            "Kode dan nama satker wajib diisi".into(),
        ));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM satkers WHERE LOWER(code) = LOWER($1) AND id <> $2)",
    )
    .bind(code)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if dup {
        return Err(AppError::BadRequest(format!(
            "Kode satker '{code}' sudah digunakan"
        )));
    }
    let row = sqlx::query(
        r#"
        UPDATE satkers
        SET code = $1, name = $2, is_active = $3, updated_at = NOW()
        WHERE id = $4
        RETURNING id, code, name, is_active, created_by, created_at, updated_at
        "#,
    )
    .bind(req.code.trim())
    .bind(req.name.trim())
    .bind(req.is_active)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("satker not found".into()))?;

    Ok(Json(ApiResponse {
        data: map_satker_row(&row)?,
    }))
}

pub async fn delete_satker(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_super_admin_write()?;
    let res = sqlx::query("DELETE FROM satkers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("satker not found".into()));
    }
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_satker_row(row: &sqlx::postgres::PgRow) -> Result<Satker, sqlx::Error> {
    Ok(Satker {
        id: row.try_get("id")?,
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        is_active: row.try_get("is_active")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
