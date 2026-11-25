use axum::{Json, extract::State, http::HeaderMap};
use serde_json::json;
use sqlx::Row;
use twg_shared::{
    api::{
        ApiResponse, ChangePasswordRequest, LoginRequest, LoginResponse, ProfileUpdateRequest,
        RefreshRequest,
    },
    models::{UserDto, UserRole},
};
use uuid::Uuid;

use crate::{
    auth::{
        middleware::AuthUser,
        password::{hash_password, verify_password},
        token::{TokenKind, hash_refresh_token},
    },
    error::{AppError, AppResult},
    state::AppState,
};

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let username = req.username.trim();
    if username.is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest(
            "username and password are required".into(),
        ));
    }

    let device_id = resolve_device_id(&headers, req.device_id.as_deref())?;

    let row = sqlx::query(
        r#"
        SELECT u.id, u.name, u.username, u.password, u.role, u.satker_id, u.profile_img, u.units,
               u.created_at, u.updated_at, s.name AS satker_name
        FROM users u
        LEFT JOIN satkers s ON s.id = u.satker_id
        WHERE u.username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid username or password".into()))?;

    let password_hash: String = row.try_get("password")?;
    if !verify_password(&req.password, &password_hash) {
        return Err(AppError::Unauthorized(
            "invalid username or password".into(),
        ));
    }

    let user = map_user_row(&row)?;
    let (access_token, refresh_token) =
        state
            .tokens
            .issue_pair(user.id, &user.username, user.role, user.satker_id)?;

    upsert_session(
        &state,
        user.id,
        &device_id,
        &refresh_token,
        matches!(user.role, UserRole::Admin | UserRole::AdminSatker),
    )
    .await?;

    Ok(Json(ApiResponse {
        data: LoginResponse {
            access_token,
            refresh_token,
            user,
        },
    }))
}

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RefreshRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let device_id = resolve_device_id(&headers, req.device_id.as_deref())?;
    let payload = state
        .tokens
        .verify(&req.refresh_token, TokenKind::Refresh)
        .map_err(|_| AppError::Unauthorized("invalid or expired refresh token".into()))?;

    let token_hash = hash_refresh_token(&req.refresh_token);
    let session = sqlx::query(
        r#"
        SELECT id FROM user_sessions
        WHERE user_id = $1 AND device_id = $2 AND refresh_token_hash = $3 AND revoked_at IS NULL
        "#,
    )
    .bind(payload.user_id)
    .bind(&device_id)
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?;

    if session.is_none() {
        return Err(AppError::Unauthorized("refresh session revoked".into()));
    }

    let user = fetch_user_by_id(&state, payload.user_id).await?;
    let (access_token, refresh_token) =
        state
            .tokens
            .issue_pair(user.id, &user.username, user.role, user.satker_id)?;

    upsert_session(
        &state,
        user.id,
        &device_id,
        &refresh_token,
        matches!(user.role, UserRole::Admin | UserRole::AdminSatker),
    )
    .await?;

    Ok(Json(ApiResponse {
        data: LoginResponse {
            access_token,
            refresh_token,
            user,
        },
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sqlx::query(
        "UPDATE user_sessions SET revoked_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND device_id = $2",
    )
    .bind(auth.user_id)
    .bind(&auth.device_id)
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: json!({ "logged_out": true }),
    }))
}

pub async fn identify(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    let user = fetch_user_by_id(&state, auth.user_id).await?;
    Ok(Json(ApiResponse { data: user }))
}

pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ProfileUpdateRequest>,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Nama lengkap tidak boleh kosong".into()));
    }

    sqlx::query(
        "UPDATE users SET name = $1, profile_img = COALESCE($2, profile_img), updated_at = NOW() WHERE id = $3",
    )
    .bind(req.name.trim())
    .bind(req.profile_img)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    let user = fetch_user_by_id(&state, auth.user_id).await?;
    Ok(Json(ApiResponse { data: user }))
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    if req.new_password.len() < 6 {
        return Err(AppError::BadRequest(
            "Password baru minimal 6 karakter".into(),
        ));
    }

    let row = sqlx::query("SELECT password FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User tidak ditemukan".into()))?;
    let current_hash: String = row.try_get("password")?;

    if !verify_password(&req.old_password, &current_hash) {
        return Err(AppError::Unauthorized("Password lama tidak sesuai".into()));
    }

    let new_hash = hash_password(&req.new_password)?;
    sqlx::query("UPDATE users SET password = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hash)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: json!({ "password_updated": true }),
    }))
}

async fn upsert_session(
    state: &AppState,
    user_id: Uuid,
    device_id: &str,
    refresh_token: &str,
    single_device: bool,
) -> AppResult<()> {
    let hash = hash_refresh_token(refresh_token);
    if single_device {
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND device_id <> $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&state.db)
        .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO user_sessions (user_id, device_id, refresh_token_hash, is_single_device, last_used_at, revoked_at)
        VALUES ($1, $2, $3, $4, NOW(), NULL)
        ON CONFLICT (user_id, device_id)
        DO UPDATE SET
            refresh_token_hash = EXCLUDED.refresh_token_hash,
            is_single_device = EXCLUDED.is_single_device,
            last_used_at = NOW(),
            revoked_at = NULL,
            updated_at = NOW()
        "#,
    )
    .bind(user_id)
    .bind(device_id)
    .bind(hash)
    .bind(single_device)
    .execute(&state.db)
    .await?;

    Ok(())
}

pub async fn fetch_user_by_id(state: &AppState, id: Uuid) -> AppResult<UserDto> {
    let row = sqlx::query(
        r#"
        SELECT u.id, u.name, u.username, u.role, u.satker_id, u.profile_img, u.units,
               u.created_at, u.updated_at, s.name AS satker_name
        FROM users u
        LEFT JOIN satkers s ON s.id = u.satker_id
        WHERE u.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    Ok(map_user_row(&row)?)
}

pub fn map_user_row(row: &sqlx::postgres::PgRow) -> Result<UserDto, sqlx::Error> {
    let role_str: String = row.try_get("role")?;
    Ok(UserDto {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        username: row.try_get("username")?,
        role: UserRole::from_db(&role_str),
        satker_id: row.try_get("satker_id")?,
        satker_name: row.try_get("satker_name")?,
        profile_img: row.try_get("profile_img")?,
        units: row.try_get("units")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn resolve_device_id(headers: &HeaderMap, body_device: Option<&str>) -> AppResult<String> {
    if let Some(h) = headers.get("x-device-id").and_then(|v| v.to_str().ok()) {
        let t = h.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Some(b) = body_device {
        let t = b.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    Ok("desktop-default".to_string())
}
