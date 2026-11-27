use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use twg_shared::{
    api::{ApiResponse, CreateUserRequest, UpdateUserAssetsRequest, UpdateUserRequest},
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

pub async fn fetch_user_allowed_assets(
    db: &PgPool,
    user_id: Uuid,
) -> Result<(Vec<Uuid>, Vec<String>), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT ma.id, ma.model_path
        FROM user_model_assets uma
        JOIN model_assets ma ON ma.id = uma.model_asset_id
        WHERE uma.user_id = $1
        ORDER BY ma.category ASC, ma.name ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut ids = Vec::with_capacity(rows.len());
    let mut paths = Vec::with_capacity(rows.len());
    for r in rows {
        ids.push(r.try_get("id")?);
        paths.push(r.try_get("model_path")?);
    }
    Ok((ids, paths))
}

pub async fn attach_allowed_assets_to_users(
    db: &PgPool,
    users: &mut [UserDto],
) -> Result<(), sqlx::Error> {
    if users.is_empty() {
        return Ok(());
    }
    let user_ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
    let rows = sqlx::query(
        r#"
        SELECT uma.user_id, ma.id AS model_asset_id, ma.model_path
        FROM user_model_assets uma
        JOIN model_assets ma ON ma.id = uma.model_asset_id
        WHERE uma.user_id = ANY($1)
        ORDER BY ma.category ASC, ma.name ASC
        "#,
    )
    .bind(&user_ids)
    .fetch_all(db)
    .await?;

    let mut by_user: HashMap<Uuid, (Vec<Uuid>, Vec<String>)> = HashMap::new();
    for r in rows {
        let uid: Uuid = r.try_get("user_id")?;
        let mid: Uuid = r.try_get("model_asset_id")?;
        let mpath: String = r.try_get("model_path")?;
        let entry = by_user.entry(uid).or_default();
        entry.0.push(mid);
        entry.1.push(mpath);
    }

    for u in users.iter_mut() {
        if let Some((ids, paths)) = by_user.remove(&u.id) {
            u.allowed_model_asset_ids = ids;
            u.allowed_model_paths = paths;
        } else {
            u.allowed_model_asset_ids.clear();
            u.allowed_model_paths.clear();
        }
    }
    Ok(())
}

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

    let mut items = rows
        .iter()
        .map(map_user_row)
        .collect::<Result<Vec<_>, _>>()?;
    attach_allowed_assets_to_users(&state.db, &mut items).await?;
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
    let name = req.name.trim();
    let username = req.username.trim();
    if name.is_empty() || username.is_empty() || req.password.len() < 6 {
        return Err(AppError::BadRequest(
            "Nama, username wajib diisi dan password minimal 6 karakter".into(),
        ));
    }

    let target_satker_id = if auth.role.is_admin_satker() {
        if !req.role.is_satker_role() {
            return Err(AppError::Forbidden(
                "Admin Satker hanya dapat membuat user dengan role Admin Satker, User Satker, atau Layar Besar Satker"
                    .into(),
            ));
        }
        let Some(sid) = auth.satker_id else {
            return Err(AppError::Forbidden(
                "Akun Admin Satker belum terikat ke Satker".into(),
            ));
        };
        Some(sid)
    } else if req.role.is_satker_role() {
        let Some(sid) = req.satker_id else {
            return Err(AppError::BadRequest(
                "Satker wajib dipilih untuk role satker".into(),
            ));
        };
        Some(sid)
    } else {
        None
    };

    let dup: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(username) = LOWER($1))")
            .bind(username)
            .fetch_one(&state.db)
            .await?;
    if dup {
        return Err(AppError::BadRequest(format!(
            "Username '{username}' sudah digunakan"
        )));
    }

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
    .bind(name)
    .bind(username)
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
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Nama user wajib diisi".into()));
    }
    let existing = fetch_user_by_id(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden(
            "cross-satker user update denied".into(),
        ));
    }

    let target_satker_id = if auth.role.is_admin_satker() {
        if !req.role.is_satker_role() {
            return Err(AppError::Forbidden(
                "Admin Satker hanya dapat menetapkan role Admin Satker, User Satker, atau Layar Besar Satker".into(),
            ));
        }
        let Some(sid) = auth.satker_id else {
            return Err(AppError::Forbidden(
                "Akun Admin Satker belum terikat ke Satker".into(),
            ));
        };
        Some(sid)
    } else if req.role.is_satker_role() {
        let Some(sid) = req.satker_id else {
            return Err(AppError::BadRequest(
                "Satker wajib dipilih untuk role satker".into(),
            ));
        };
        Some(sid)
    } else {
        None
    };

    let units = if req.units.is_null() {
        existing.units
    } else {
        req.units
    };

    let new_pw_hash = match req.password.as_deref().map(str::trim) {
        Some(pw) if !pw.is_empty() => {
            if pw.len() < 6 {
                return Err(AppError::BadRequest(
                    "Password baru minimal 6 karakter".into(),
                ));
            }
            Some(hash_password(pw)?)
        }
        _ => None,
    };

    if let Some(pw_hash) = new_pw_hash {
        sqlx::query(
            r#"
            UPDATE users
            SET name = $1, role = $2, satker_id = $3, password = $4,
                profile_img = COALESCE($5, profile_img), units = $6, updated_at = NOW()
            WHERE id = $7
            "#,
        )
        .bind(name)
        .bind(req.role.as_str())
        .bind(target_satker_id)
        .bind(pw_hash)
        .bind(req.profile_img)
        .bind(units)
        .bind(id)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE users
            SET name = $1, role = $2, satker_id = $3,
                profile_img = COALESCE($4, profile_img), units = $5, updated_at = NOW()
            WHERE id = $6
            "#,
        )
        .bind(name)
        .bind(req.role.as_str())
        .bind(target_satker_id)
        .bind(req.profile_img)
        .bind(units)
        .bind(id)
        .execute(&state.db)
        .await?;
    }

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
        return Err(AppError::BadRequest(
            "cannot delete active self account".into(),
        ));
    }
    let existing = fetch_user_by_id(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden(
            "cross-satker user delete denied".into(),
        ));
    }

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

pub async fn update_user_assets(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserAssetsRequest>,
) -> AppResult<Json<ApiResponse<UserDto>>> {
    auth.require_admin_write()?;
    let existing = fetch_user_by_id(&state, id).await?;
    if !auth.can_mutate_satker(existing.satker_id) {
        return Err(AppError::Forbidden(
            "Admin Satker hanya dapat mengatur Assets 3D untuk User Satker miliknya".into(),
        ));
    }
    if !existing.role.is_standard_user() {
        return Err(AppError::BadRequest(
            "List Assets 3D hanya berlaku untuk role User dan User Satker".into(),
        ));
    }

    let mut unique_ids: Vec<Uuid> = Vec::with_capacity(req.model_asset_ids.len());
    let mut seen: HashSet<Uuid> = HashSet::with_capacity(req.model_asset_ids.len());
    for mid in req.model_asset_ids {
        if seen.insert(mid) {
            unique_ids.push(mid);
        }
    }

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM user_model_assets WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    if !unique_ids.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO user_model_assets (user_id, model_asset_id, assigned_by)
            SELECT $1, ma.id, $2
            FROM model_assets ma
            WHERE ma.id = ANY($3)
            ON CONFLICT (user_id, model_asset_id) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(auth.user_id)
        .bind(&unique_ids)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let user = fetch_user_by_id(&state, id).await?;
    Ok(Json(ApiResponse { data: user }))
}

pub async fn list_online_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> AppResult<Json<ApiResponse<Vec<OnlineOperator>>>> {
    let operators = state.ws_hub.online_operators().await;
    Ok(Json(ApiResponse { data: operators }))
}
