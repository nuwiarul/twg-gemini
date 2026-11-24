use axum::{
    Json,
    extract::{Path, State},
};
use twg_shared::{
    api::{ApiResponse, CreateUserRequest, UpdateUserRequest},
    models::UserDto,
    ws::OnlineOperator,
};
use uuid::Uuid;

use crate::{
    auth::{middleware::AuthUser, password::hash_password},
    error::{AppError, AppResult},
    routes::auth::{fetch_user_by_id, map_user_row},
    state::AppState,
};

pub async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<ApiResponse<Vec<UserDto>>>> {
    auth.require_admin_like_read()?;

    let rows = if auth.role.is_satker_role() {
        sqlx::query(
            r#"
            SELECT u.id, u.name, u.username, u.role, u.satker_id, u.profile_img, u.units,
                   u.created_at, u.updated_at, s.name AS satker_name
            FROM users u
            LEFT JOIN satkers s ON s.id = u.satker_id
            WHERE u.satker_id = $1
            ORDER BY u.created_at DESC
            "#,
        )
        .bind(auth.satker_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT u.id, u.name, u.username, u.role, u.satker_id, u.profile_img, u.units,
                   u.created_at, u.updated_at, s.name AS satker_name
            FROM users u
            LEFT JOIN satkers s ON s.id = u.satker_id
            ORDER BY u.created_at DESC
            "#,
        )
        .fetch_all(&state.db)
        .await?
    };

    let items = rows.iter().map(map_user_row).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(ApiResponse { data: items }))
}

pub async fn get_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    if auth.user_id != id {
        auth.require_admin_like_read()?;
    }
    let user = fetch_user_by_id(&state, id).await?;
    if auth.user_id != id && !auth.can_access_satker(user.satker_id) {
        return Err(AppError::Forbidden("cross-satker access denied".into()));
    }
    Ok(Json(ApiResponse { data: user }))
}

pub async fn create_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    auth.require_admin_write()?;
    if req.username.trim().is_empty() || req.password.len() < 6 {
        return Err(AppError::BadRequest(
            "username required and password >= 6 chars".into(),
        ));
    }

    let target_satker_id = if auth.role.is_admin_satker() {
        auth.satker_id
    } else {
        req.satker_id
    };

    let pw_hash = hash_password(&req.password)?;
    let id = Uuid::new_v4();
    let units = if req.units.is_null() {
        serde_json::json!([])
    } else {
        req.units
    };

    sqlx::query(
        r#"
        INSERT INTO users (id, name, username, password, role, satker_id, profile_img, units)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(req.name.trim())
    .bind(req.username.trim())
    .bind(pw_hash)
    .bind(req.role.as_str())
    .bind(target_satker_id)
    .bind(req.profile_img)
    .bind(units)
    .execute(&state.db)
    .await?;

    let user = fetch_user_by_id(&state, id).await?;
    Ok(Json(ApiResponse { data: user }))
}

pub async fn update_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    auth.require_admin_write()?;
    let existing = fetch_user_by_id(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker user update denied".into()));
    }

    let target_satker_id = if auth.role.is_admin_satker() {
        auth.satker_id
    } else {
        req.satker_id
    };
    let units = if req.units.is_null() {
        existing.units
    } else {
        req.units
    };

    sqlx::query(
        r#"
        UPDATE users
        SET name = $1, role = $2, satker_id = $3, profile_img = COALESCE($4, profile_img), units = $5, updated_at = NOW()
        WHERE id = $6
        "#,
    )
    .bind(req.name.trim())
    .bind(req.role.as_str())
    .bind(target_satker_id)
    .bind(req.profile_img)
    .bind(units)
    .bind(id)
    .execute(&state.db)
    .await?;

    let user = fetch_user_by_id(&state, id).await?;
    Ok(Json(ApiResponse { data: user }))
}

pub async fn delete_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_admin_write()?;
    if auth.user_id == id {
        return Err(AppError::BadRequest("cannot delete active self account".into()));
    }
    let existing = fetch_user_by_id(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden("cross-satker user delete denied".into()));
    }

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

pub async fn list_online_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> AppResult<Json<ApiResponse<Vec<OnlineOperator>>>> {
    let operators = state.ws_hub.online_operators().await;
    Ok(Json(ApiResponse { data: operators }))
}
