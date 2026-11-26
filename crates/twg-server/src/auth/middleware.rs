use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use sqlx::Row;
use twg_shared::models::UserRole;
use uuid::Uuid;

use crate::{
    auth::token::{TokenKind, TokenPayload},
    error::AppError,
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    #[allow(dead_code)]
    pub username: String,
    pub role: UserRole,
    pub satker_id: Option<Uuid>,
    pub device_id: String,
}

impl AuthUser {
    pub fn require_admin_write(&self) -> Result<(), AppError> {
        if self.role.can_write_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "admin or admin_satker permission required".into(),
            ))
        }
    }

    pub fn require_super_admin_write(&self) -> Result<(), AppError> {
        if self.role.is_global_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "super admin permission required".into(),
            ))
        }
    }

    pub fn require_admin_like_read(&self) -> Result<(), AppError> {
        if self.role.is_admin_like() {
            Ok(())
        } else {
            Err(AppError::Forbidden("admin role required".into()))
        }
    }

    pub fn can_access_satker(&self, resource_satker_id: Option<Uuid>) -> bool {
        if self.role.is_global_admin() || self.role == UserRole::LayarBesar {
            return true;
        }
        if self.role.is_satker_role() {
            return self.satker_id.is_some() && self.satker_id == resource_satker_id;
        }
        // Standard user only sees unscoped (satker_id IS NULL) resources
        resource_satker_id.is_none()
    }

    pub fn can_mutate_satker(&self, resource_satker_id: Option<Uuid>) -> bool {
        if self.role.is_global_admin() {
            return true;
        }
        if self.role.is_admin_satker() {
            return self.satker_id.is_some() && self.satker_id == resource_satker_id;
        }
        false
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .ok_or_else(|| AppError::Unauthorized("invalid bearer token format".into()))?
            .trim();

        let payload: TokenPayload = state
            .tokens
            .verify(token, TokenKind::Access)
            .map_err(|_| AppError::Unauthorized("invalid or expired access token".into()))?;

        let device_id = parts
            .headers
            .get("x-device-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Unauthorized("X-Device-ID header is required".into()))?
            .to_string();

        validate_active_session(state, payload.user_id, &device_id).await?;

        Ok(Self {
            user_id: payload.user_id,
            username: payload.username,
            role: payload.role,
            satker_id: payload.satker_id,
            device_id,
        })
    }
}

pub async fn validate_active_session(
    state: &AppState,
    user_id: Uuid,
    device_id: &str,
) -> Result<(), AppError> {
    let row = sqlx::query(
        r#"
        UPDATE user_sessions
        SET last_used_at = NOW(), updated_at = NOW()
        WHERE user_id = $1 AND device_id = $2 AND revoked_at IS NULL
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_optional(&state.db)
    .await?;

    if row.is_none() {
        return Err(AppError::Unauthorized(
            "session revoked or not found; please login again".into(),
        ));
    }
    Ok(())
}

pub async fn verify_skenario_access(
    state: &AppState,
    user: &AuthUser,
    skenario_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let row = sqlx::query("SELECT satker_id FROM skenarios WHERE id = $1")
        .bind(skenario_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("skenario not found".into()))?;

    let satker_id: Option<Uuid> = row.try_get("satker_id")?;
    if !user.can_access_satker(satker_id) {
        return Err(AppError::Forbidden(
            "cross-satker access is not allowed".into(),
        ));
    }
    Ok(satker_id)
}
