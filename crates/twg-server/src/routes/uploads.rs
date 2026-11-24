use axum::{
    Json,
    extract::{Multipart, State},
};
use twg_shared::{
    api::{ApiResponse, UploadResponse},
    models::ModelCatalogEntry,
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    state::AppState,
};

pub async fn upload_file(
    State(state): State<AppState>,
    _auth: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<ApiResponse<UploadResponse>>> {
    tokio::fs::create_dir_all(&state.config.upload_dir)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart field: {e}")))?
    {
        let original = field
            .file_name()
            .unwrap_or("upload.bin")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("failed to read upload bytes: {e}")))?;

        let filename = format!("{}-{}", Uuid::new_v4(), original);
        let target = state.config.upload_dir.join(&filename);
        tokio::fs::write(&target, &data)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        return Ok(Json(ApiResponse {
            data: UploadResponse {
                url: format!("/uploads/{filename}"),
                filename,
                size_bytes: data.len(),
            },
        }));
    }

    Err(AppError::BadRequest("no file field provided".into()))
}

pub async fn get_model_catalog(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<Vec<ModelCatalogEntry>>>> {
    if let Ok(models) = crate::routes::model_assets::fetch_all_models(&state.db).await
        && !models.is_empty()
    {
        let items = models
            .into_iter()
            .map(|m| m.to_catalog_entry(true))
            .collect();
        return Ok(Json(ApiResponse { data: items }));
    }

    let catalog_path = state.config.assets_dir.join("catalog").join("models.json");
    let raw = tokio::fs::read_to_string(&catalog_path).await.map_err(|e| {
        AppError::NotFound(format!(
            "catalog not found at {}: {e}",
            catalog_path.display()
        ))
    })?;
    let items: Vec<ModelCatalogEntry> =
        serde_json::from_str(&raw).map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(ApiResponse { data: items }))
}
