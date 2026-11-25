use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::Row;
use twg_shared::{
    api::{ApiResponse, CreateOperasiRequest, PaginatedResponse, UpdateOperasiRequest},
    models::{Operasi, Skenario, TacticalOperasiCard},
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    routes::skenarios::map_skenario_row,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
}

pub async fn list_operasis(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<PaginationParams>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<Operasi>>>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(10).clamp(1, 200);
    let offset = (page - 1) * per_page;
    let search_pattern = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let (rows, total) = if auth.role.is_satker_role() {
        let rows = sqlx::query(
            r#"
            SELECT o.id, o.name, o.description, o.satker_id, o.created_at, o.updated_at,
                   s.name AS satker_name,
                   (SELECT COUNT(*) FROM skenarios sk WHERE sk.operasi_id = o.id) AS skenario_count
            FROM operasis o
            LEFT JOIN satkers s ON s.id = o.satker_id
            WHERE o.satker_id = $1
              AND ($2::text IS NULL OR o.name ILIKE $2 OR COALESCE(o.description, '') ILIKE $2 OR COALESCE(s.name, '') ILIKE $2)
            ORDER BY o.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(auth.satker_id)
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM operasis o
            LEFT JOIN satkers s ON s.id = o.satker_id
            WHERE o.satker_id = $1
              AND ($2::text IS NULL OR o.name ILIKE $2 OR COALESCE(o.description, '') ILIKE $2 OR COALESCE(s.name, '') ILIKE $2)
            "#,
        )
        .bind(auth.satker_id)
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    } else if auth.role.as_str() == "user" {
        let rows = sqlx::query(
            r#"
            SELECT o.id, o.name, o.description, o.satker_id, o.created_at, o.updated_at,
                   s.name AS satker_name,
                   (SELECT COUNT(*) FROM skenarios sk WHERE sk.operasi_id = o.id) AS skenario_count
            FROM operasis o
            LEFT JOIN satkers s ON s.id = o.satker_id
            WHERE o.satker_id IS NULL
              AND ($1::text IS NULL OR o.name ILIKE $1 OR COALESCE(o.description, '') ILIKE $1)
            ORDER BY o.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM operasis o
            WHERE o.satker_id IS NULL
              AND ($1::text IS NULL OR o.name ILIKE $1 OR COALESCE(o.description, '') ILIKE $1)
            "#,
        )
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    } else {
        let rows = sqlx::query(
            r#"
            SELECT o.id, o.name, o.description, o.satker_id, o.created_at, o.updated_at,
                   s.name AS satker_name,
                   (SELECT COUNT(*) FROM skenarios sk WHERE sk.operasi_id = o.id) AS skenario_count
            FROM operasis o
            LEFT JOIN satkers s ON s.id = o.satker_id
            WHERE ($1::text IS NULL OR o.name ILIKE $1 OR COALESCE(o.description, '') ILIKE $1 OR COALESCE(s.name, '') ILIKE $1)
            ORDER BY o.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(&search_pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let cnt: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM operasis o
            LEFT JOIN satkers s ON s.id = o.satker_id
            WHERE ($1::text IS NULL OR o.name ILIKE $1 OR COALESCE(o.description, '') ILIKE $1 OR COALESCE(s.name, '') ILIKE $1)
            "#,
        )
        .bind(&search_pattern)
        .fetch_one(&state.db)
        .await?;
        (rows, cnt)
    };

    let items = rows
        .iter()
        .map(map_operasi_row)
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

/// Returns paginated Operasis with each Operasi's full list of Skenarios embedded for the
/// Tactical / Simulasi cards page (respecting exact role & satker scoping).
pub async fn list_tactical_operasis(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<PaginationParams>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<TacticalOperasiCard>>>> {
    let Json(ApiResponse { data: op_page }) =
        list_operasis(State(state.clone()), auth, Query(params)).await?;

    let op_ids: Vec<Uuid> = op_page.items.iter().map(|o| o.id).collect();
    let mut sk_by_op: HashMap<Uuid, Vec<Skenario>> = HashMap::new();

    if !op_ids.is_empty() {
        let sk_rows = sqlx::query(
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
            WHERE sk.operasi_id = ANY($1)
            ORDER BY sk.created_at DESC
            "#,
        )
        .bind(&op_ids)
        .fetch_all(&state.db)
        .await?;

        for row in &sk_rows {
            let sk = map_skenario_row(row)?;
            sk_by_op.entry(sk.operasi_id).or_default().push(sk);
        }
    }

    let cards = op_page
        .items
        .into_iter()
        .map(|operasi| {
            let skenarios = sk_by_op.remove(&operasi.id).unwrap_or_default();
            TacticalOperasiCard { operasi, skenarios }
        })
        .collect();

    Ok(Json(ApiResponse {
        data: PaginatedResponse {
            items: cards,
            total: op_page.total,
            page: op_page.page,
            per_page: op_page.per_page,
        },
    }))
}

pub async fn get_operasi(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Operasi>>> {
    let op = fetch_operasi(&state, id).await?;
    if !auth.can_access_satker(op.satker_id) {
        return Err(AppError::Forbidden("cross-satker access denied".into()));
    }
    Ok(Json(ApiResponse { data: op }))
}

pub async fn create_operasi(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateOperasiRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Operasi>>)> {
    auth.require_admin_write()?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let satker_id = if auth.role.is_admin_satker() {
        auth.satker_id
    } else {
        req.satker_id
    };

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO operasis (id, name, description, satker_id) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(req.name.trim())
        .bind(req.description)
        .bind(satker_id)
        .execute(&state.db)
        .await?;

    let op = fetch_operasi(&state, id).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse { data: op })))
}

pub async fn update_operasi(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOperasiRequest>,
) -> AppResult<Json<ApiResponse<Operasi>>> {
    auth.require_admin_write()?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let existing = fetch_operasi(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker update denied".into()));
    }

    let satker_id = if auth.role.is_admin_satker() {
        auth.satker_id
    } else {
        req.satker_id
    };

    sqlx::query(
        "UPDATE operasis SET name = $1, description = $2, satker_id = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(req.name.trim())
    .bind(req.description)
    .bind(satker_id)
    .bind(id)
    .execute(&state.db)
    .await?;

    let op = fetch_operasi(&state, id).await?;
    Ok(Json(ApiResponse { data: op }))
}

pub async fn delete_operasi(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_admin_write()?;
    let existing = fetch_operasi(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker delete denied".into()));
    }

    let skenario_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM skenarios WHERE operasi_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    if skenario_count > 0 {
        return Err(AppError::BadRequest(format!(
            "Operasi tidak dapat dihapus karena masih memiliki {skenario_count} skenario di dalamnya. Hapus atau pindahkan semua skenario terlebih dahulu."
        )));
    }

    sqlx::query("DELETE FROM operasis WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

async fn fetch_operasi(state: &AppState, id: Uuid) -> AppResult<Operasi> {
    let row = sqlx::query(
        r#"
        SELECT o.id, o.name, o.description, o.satker_id, o.created_at, o.updated_at,
               s.name AS satker_name,
               (SELECT COUNT(*) FROM skenarios sk WHERE sk.operasi_id = o.id) AS skenario_count
        FROM operasis o
        LEFT JOIN satkers s ON s.id = o.satker_id
        WHERE o.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("operasi not found".into()))?;

    Ok(map_operasi_row(&row)?)
}

fn map_operasi_row(row: &sqlx::postgres::PgRow) -> Result<Operasi, sqlx::Error> {
    Ok(Operasi {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        satker_id: row.try_get("satker_id")?,
        satker_name: row.try_get("satker_name")?,
        skenario_count: row.try_get("skenario_count").unwrap_or(0),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
