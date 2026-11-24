use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use twg_shared::api::ApiErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("internal server error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Self::NotFound("record not found".into()),
            sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
                Self::Conflict(format!("duplicate record: {}", db_err.message()))
            }
            sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23503") => {
                Self::BadRequest(format!("invalid reference: {}", db_err.message()))
            }
            _ => Self::Internal(err.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(err) => {
                tracing::error!(?err, "unhandled internal error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        let message = match &self {
            Self::Internal(_) => "internal server error".to_string(),
            other => other.to_string(),
        };

        let body = ApiErrorBody {
            code: status.as_u16(),
            message,
        };
        (status, Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
