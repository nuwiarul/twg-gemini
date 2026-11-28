use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use anyhow::Context;
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, Query, State},
};
use chrono::{DateTime, Utc};
use image::{ColorType, DynamicImage, ImageFormat, imageops::FilterType};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::process::Command;
use twg_shared::{
    api::{ApiResponse, ResizeTextureRequest},
    models::{MediaAssetDto, MediaAssetType, ModelFileMeta},
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListMediaQuery {
    pub media_type: Option<String>,
    pub search: Option<String>,
}

struct UploadedMediaFile {
    filename: String,
    bytes: Vec<u8>,
}

struct ParsedMediaForm {
    fields: HashMap<String, String>,
    file: Option<UploadedMediaFile>,
}

fn asset_download_url(rel_path: &str) -> String {
    format!(
        "/api/v1/assets/models/file/{}",
        rel_path.trim_start_matches('/')
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn row_to_media_dto(row: &sqlx::postgres::PgRow) -> AppResult<MediaAssetDto> {
    let original_path: String = row.try_get("original_path")?;
    let original_sha256: String = row.try_get("original_sha256")?;
    let original_size_bytes: i64 = row.try_get("original_size_bytes")?;

    let active_path: String = row.try_get("active_path")?;
    let active_sha256: String = row.try_get("active_sha256")?;
    let active_size_bytes: i64 = row.try_get("active_size_bytes")?;

    let thumbnail_path: Option<String> = row.try_get("thumbnail_path")?;
    let thumbnail_sha256: Option<String> = row.try_get("thumbnail_sha256")?;
    let thumbnail_size_bytes: i64 = row.try_get("thumbnail_size_bytes")?;

    let thumbnail_file = match (thumbnail_path, thumbnail_sha256) {
        (Some(rel), Some(sha)) if !rel.is_empty() && !sha.is_empty() => Some(ModelFileMeta {
            download_url: asset_download_url(&rel),
            rel_path: rel,
            sha256: sha,
            size_bytes: thumbnail_size_bytes,
            tris: 0,
        }),
        _ => None,
    };

    let media_type_str: String = row.try_get("media_type")?;

    Ok(MediaAssetDto {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        media_type: MediaAssetType::from_db(&media_type_str),
        category: row.try_get("category")?,
        operasi_id: row.try_get("operasi_id").ok().flatten(),
        created_by: row.try_get("created_by").ok().flatten(),
        mime_type: row.try_get("mime_type")?,
        original_file: ModelFileMeta {
            download_url: asset_download_url(&original_path),
            rel_path: original_path,
            sha256: original_sha256,
            size_bytes: original_size_bytes,
            tris: 0,
        },
        active_file: ModelFileMeta {
            download_url: asset_download_url(&active_path),
            rel_path: active_path,
            sha256: active_sha256,
            size_bytes: active_size_bytes,
            tris: 0,
        },
        thumbnail_file,
        original_width: row.try_get("original_width")?,
        original_height: row.try_get("original_height")?,
        active_width: row.try_get("active_width")?,
        active_height: row.try_get("active_height")?,
        duration_sec: row.try_get("duration_sec")?,
        crop_mode: row.try_get("crop_mode")?,
        version: row.try_get("version")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

pub async fn fetch_all_media(db: &PgPool) -> AppResult<Vec<MediaAssetDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, slug, name, media_type, category, operasi_id, created_by, mime_type,
            original_path, original_sha256, original_size_bytes, original_width, original_height,
            active_path, active_sha256, active_size_bytes, active_width, active_height,
            thumbnail_path, thumbnail_sha256, thumbnail_size_bytes,
            duration_sec, crop_mode, version, created_at, updated_at
        FROM media_assets
        ORDER BY media_type ASC, name ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    rows.iter().map(row_to_media_dto).collect()
}

async fn fetch_media_by_id(db: &PgPool, id: Uuid) -> AppResult<MediaAssetDto> {
    let row = sqlx::query(
        r#"
        SELECT
            id, slug, name, media_type, category, operasi_id, created_by, mime_type,
            original_path, original_sha256, original_size_bytes, original_width, original_height,
            active_path, active_sha256, active_size_bytes, active_width, active_height,
            thumbnail_path, thumbnail_sha256, thumbnail_size_bytes,
            duration_sec, crop_mode, version, created_at, updated_at
        FROM media_assets
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("media asset tidak ditemukan".into()))?;

    row_to_media_dto(&row)
}

pub async fn list_media_assets(
    State(state): State<AppState>,
    Query(q): Query<ListMediaQuery>,
) -> AppResult<Json<ApiResponse<Vec<MediaAssetDto>>>> {
    let mut items = fetch_all_media(&state.db).await?;
    if let Some(mt) = q
        .media_type
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "all")
    {
        items.retain(|m| m.media_type.as_str() == mt);
    }
    if let Some(search) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let needle = search.to_lowercase();
        items.retain(|m| {
            m.name.to_lowercase().contains(&needle)
                || m.slug.to_lowercase().contains(&needle)
                || m.category.to_lowercase().contains(&needle)
        });
    }
    Ok(Json(ApiResponse { data: items }))
}

pub async fn get_media_asset(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> AppResult<Json<ApiResponse<MediaAssetDto>>> {
    let dto = fetch_media_by_id(&state.db, id).await?;
    Ok(Json(ApiResponse { data: dto }))
}

fn media_folder_prefix(operasi_id: Option<Uuid>, sub: &str) -> String {
    match operasi_id {
        Some(op_id) => format!("media/operasi_{}/{sub}", op_id.simple()),
        None => format!("media/{sub}"),
    }
}

fn media_slug_prefix(operasi_id: Option<Uuid>, media_type: MediaAssetType) -> String {
    match operasi_id {
        Some(op_id) => format!("op_{}/{}", op_id.simple(), media_type.as_str()),
        None => media_type.as_str().to_string(),
    }
}

async fn broadcast_media_assets_changed(
    state: &AppState,
    operasi_id: Option<Uuid>,
    asset_id: Uuid,
    actor_username: &str,
) {
    state
        .ws_hub
        .broadcast(
            crate::ws::HubDelivery::All,
            twg_shared::ws::WsServerMessage::AssetsChanged {
                operasi_id,
                kind: "media".into(),
                asset_id,
                actor_username: actor_username.to_string(),
            },
        )
        .await;
}

pub async fn create_media_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> AppResult<Json<ApiResponse<MediaAssetDto>>> {
    let form = parse_media_multipart(multipart).await?;
    let operasi_id = match form
        .fields
        .get("operasi_id")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(
            Uuid::parse_str(raw)
                .map_err(|_| AppError::BadRequest("operasi_id tidak valid".into()))?,
        ),
        None => None,
    };
    crate::auth::middleware::verify_operasi_asset_upload(&state, &auth, operasi_id).await?;

    let name = form
        .fields
        .get("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Nama asset wajib diisi".into()))?;

    let media_type_raw = form
        .fields
        .get("media_type")
        .map(|s| s.trim().to_lowercase())
        .ok_or_else(|| {
            AppError::BadRequest("media_type wajib diisi (texture, sound, video)".into())
        })?;
    let media_type = match media_type_raw.as_str() {
        "texture" | "image" => MediaAssetType::Texture,
        "sound" | "audio" => MediaAssetType::Sound,
        "video" => MediaAssetType::Video,
        _ => {
            return Err(AppError::BadRequest(
                "media_type harus 'texture', 'sound', atau 'video'".into(),
            ));
        }
    };

    let category = form
        .fields
        .get("category")
        .map(|s| sanitize_slug(s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "general".to_string());

    let crop_mode_raw = form
        .fields
        .get("crop_mode")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "crop".to_string());
    let crop_mode = if crop_mode_raw == "resize" {
        "resize".to_string()
    } else {
        "crop".to_string()
    };

    let upload = form
        .file
        .ok_or_else(|| AppError::BadRequest("File media wajib diunggah".into()))?;

    let base = sanitize_slug(&name);
    let slug_prefix = media_slug_prefix(operasi_id, media_type);
    let unique_base = make_unique_media_base(&state.db, &slug_prefix, &base, None).await?;
    let slug = format!("{slug_prefix}/{unique_base}");
    let assets_dir = &state.config.assets_dir;
    let id = Uuid::new_v4();

    let processed = process_and_store_media(
        assets_dir,
        operasi_id,
        media_type,
        &unique_base,
        &upload,
        &crop_mode,
    )
    .await?;

    sqlx::query(
        r#"
        INSERT INTO media_assets (
            id, slug, name, media_type, category, operasi_id, mime_type,
            original_path, original_sha256, original_size_bytes, original_width, original_height,
            active_path, active_sha256, active_size_bytes, active_width, active_height,
            thumbnail_path, thumbnail_sha256, thumbnail_size_bytes,
            duration_sec, crop_mode, version, created_by
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17,
            $18, $19, $20,
            $21, $22, 1, $23
        )
        "#,
    )
    .bind(id)
    .bind(&slug)
    .bind(&name)
    .bind(media_type.as_str())
    .bind(&category)
    .bind(operasi_id)
    .bind(&processed.mime_type)
    .bind(&processed.rel_path)
    .bind(&processed.sha256)
    .bind(processed.size_bytes)
    .bind(processed.width)
    .bind(processed.height)
    .bind(&processed.rel_path)
    .bind(&processed.sha256)
    .bind(processed.size_bytes)
    .bind(processed.width)
    .bind(processed.height)
    .bind(&processed.thumb_path)
    .bind(&processed.thumb_sha256)
    .bind(processed.thumb_size_bytes)
    .bind(processed.duration_sec)
    .bind(if media_type == MediaAssetType::Texture {
        Some(crop_mode.as_str())
    } else {
        None
    })
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    let dto = fetch_media_by_id(&state.db, id).await?;
    broadcast_media_assets_changed(&state, operasi_id, id, &auth.username).await;
    Ok(Json(ApiResponse { data: dto }))
}

pub async fn update_media_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    multipart: Multipart,
) -> AppResult<Json<ApiResponse<MediaAssetDto>>> {
    let existing = fetch_media_by_id(&state.db, id).await?;
    crate::auth::middleware::verify_operasi_asset_mutation(
        &state,
        &auth,
        existing.operasi_id,
        existing.created_by,
    )
    .await?;
    let form = parse_media_multipart(multipart).await?;

    let name = form
        .fields
        .get("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| existing.name.clone());

    let category = form
        .fields
        .get("category")
        .map(|s| sanitize_slug(s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| existing.category.clone());

    let crop_mode_raw = form
        .fields
        .get("crop_mode")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| {
            existing
                .crop_mode
                .clone()
                .unwrap_or_else(|| "crop".to_string())
        });
    let crop_mode = if crop_mode_raw == "resize" {
        "resize".to_string()
    } else {
        "crop".to_string()
    };

    let assets_dir = &state.config.assets_dir;
    let base = existing
        .slug
        .rsplit('/')
        .next()
        .unwrap_or("media")
        .to_string();

    if let Some(upload) = form.file {
        let processed = process_and_store_media(
            assets_dir,
            existing.operasi_id,
            existing.media_type,
            &base,
            &upload,
            &crop_mode,
        )
        .await?;

        let mut cleanup = Vec::new();
        if existing.original_file.rel_path != processed.rel_path {
            cleanup.push(existing.original_file.rel_path.clone());
        }
        if existing.active_file.rel_path != existing.original_file.rel_path
            && existing.active_file.rel_path != processed.rel_path
        {
            cleanup.push(existing.active_file.rel_path.clone());
        }
        if let Some(old_thumb) = &existing.thumbnail_file
            && processed.thumb_path.as_deref() != Some(old_thumb.rel_path.as_str())
        {
            cleanup.push(old_thumb.rel_path.clone());
        }

        sqlx::query(
            r#"
            UPDATE media_assets
            SET
                name = $2,
                category = $3,
                mime_type = $4,
                original_path = $5,
                original_sha256 = $6,
                original_size_bytes = $7,
                original_width = $8,
                original_height = $9,
                active_path = $10,
                active_sha256 = $11,
                active_size_bytes = $12,
                active_width = $13,
                active_height = $14,
                thumbnail_path = $15,
                thumbnail_sha256 = $16,
                thumbnail_size_bytes = $17,
                duration_sec = $18,
                crop_mode = $19,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(&name)
        .bind(&category)
        .bind(&processed.mime_type)
        .bind(&processed.rel_path)
        .bind(&processed.sha256)
        .bind(processed.size_bytes)
        .bind(processed.width)
        .bind(processed.height)
        .bind(&processed.rel_path)
        .bind(&processed.sha256)
        .bind(processed.size_bytes)
        .bind(processed.width)
        .bind(processed.height)
        .bind(&processed.thumb_path)
        .bind(&processed.thumb_sha256)
        .bind(processed.thumb_size_bytes)
        .bind(processed.duration_sec)
        .bind(if existing.media_type == MediaAssetType::Texture {
            Some(crop_mode.as_str())
        } else {
            None
        })
        .execute(&state.db)
        .await?;

        for old_rel in cleanup {
            let _ = tokio::fs::remove_file(assets_dir.join(&old_rel)).await;
        }
    } else {
        sqlx::query(
            r#"
            UPDATE media_assets
            SET name = $2, category = $3, version = version + 1, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(&name)
        .bind(&category)
        .execute(&state.db)
        .await?;
    }

    let dto = fetch_media_by_id(&state.db, id).await?;
    broadcast_media_assets_changed(&state, existing.operasi_id, id, &auth.username).await;
    Ok(Json(ApiResponse { data: dto }))
}

/// Non-destructive symmetric downscale-only resize for Image Textures.
/// NEVER deletes `original_path` and always resamples directly from `original_path`.
pub async fn resize_media_texture(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<ResizeTextureRequest>,
) -> AppResult<Json<ApiResponse<MediaAssetDto>>> {
    let existing = fetch_media_by_id(&state.db, id).await?;
    crate::auth::middleware::verify_operasi_asset_mutation(
        &state,
        &auth,
        existing.operasi_id,
        existing.created_by,
    )
    .await?;
    if existing.media_type != MediaAssetType::Texture {
        return Err(AppError::BadRequest(
            "Resize simetri hanya berlaku untuk asset tipe Image Texture".into(),
        ));
    }

    let orig_size = existing
        .original_width
        .unwrap_or(0)
        .min(existing.original_height.unwrap_or(0)) as u32;
    if orig_size == 0 {
        return Err(AppError::BadRequest(
            "Dimensi original texture tidak ditemukan".into(),
        ));
    }

    let target = req.target_size;
    if target < 16 {
        return Err(AppError::BadRequest(
            "Ukuran resize minimal adalah 16x16 pixel".into(),
        ));
    }
    if target > orig_size {
        return Err(AppError::BadRequest(format!(
            "Ukuran resize ({target}x{target}) tidak boleh lebih besar dari ukuran original ({orig_size}x{orig_size}). Resize texture hanya boleh ke bawah."
        )));
    }

    let assets_dir = &state.config.assets_dir;
    let old_active_rel = existing.active_file.rel_path.clone();
    let orig_rel = existing.original_file.rel_path.clone();

    if target == orig_size {
        // Restore original (never deleted!)
        sqlx::query(
            r#"
            UPDATE media_assets
            SET
                active_path = original_path,
                active_sha256 = original_sha256,
                active_size_bytes = original_size_bytes,
                active_width = original_width,
                active_height = original_height,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&state.db)
        .await?;

        if old_active_rel != orig_rel {
            let _ = tokio::fs::remove_file(assets_dir.join(&old_active_rel)).await;
        }
        let dto = fetch_media_by_id(&state.db, id).await?;
        broadcast_media_assets_changed(&state, existing.operasi_id, id, &auth.username).await;
        return Ok(Json(ApiResponse { data: dto }));
    }

    // Read strictly from original_path so repeated resizes never lose quality
    let orig_abs = assets_dir.join(&orig_rel);
    let orig_bytes = tokio::fs::read(&orig_abs)
        .await
        .map_err(|e| AppError::NotFound(format!("File texture original tidak ditemukan: {e}")))?;

    let is_png = existing.mime_type == "image/png" || orig_rel.to_lowercase().ends_with(".png");
    let encoded = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let img = image::load_from_memory(&orig_bytes)
            .map_err(|e| format!("Gagal decode texture original: {e}"))?;
        let resized = img.resize_exact(target, target, FilterType::Lanczos3);
        encode_texture_image(&resized, is_png)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .map_err(AppError::BadRequest)?;

    let ext = if is_png { "png" } else { "jpg" };
    let base = existing
        .slug
        .rsplit('/')
        .next()
        .unwrap_or("texture")
        .to_string();
    let tex_dir = media_folder_prefix(existing.operasi_id, "textures");
    let new_active_rel = format!("{tex_dir}/{base}_{target}x{target}.{ext}");
    let new_active_abs = assets_dir.join(&new_active_rel);
    if let Some(parent) = new_active_abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create media directory '{}'", parent.display()))
            .map_err(AppError::Internal)?;
    }
    tokio::fs::write(&new_active_abs, &encoded)
        .await
        .with_context(|| {
            format!(
                "failed to write resized texture '{}'",
                new_active_abs.display()
            )
        })
        .map_err(AppError::Internal)?;

    let new_sha = sha256_hex(&encoded);
    let new_size = encoded.len() as i64;

    sqlx::query(
        r#"
        UPDATE media_assets
        SET
            active_path = $2,
            active_sha256 = $3,
            active_size_bytes = $4,
            active_width = $5,
            active_height = $6,
            version = version + 1,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&new_active_rel)
    .bind(&new_sha)
    .bind(new_size)
    .bind(target as i32)
    .bind(target as i32)
    .execute(&state.db)
    .await?;

    // Clean up older resized variant (NEVER delete original_path!)
    if old_active_rel != orig_rel && old_active_rel != new_active_rel {
        let _ = tokio::fs::remove_file(assets_dir.join(&old_active_rel)).await;
    }

    let dto = fetch_media_by_id(&state.db, id).await?;
    broadcast_media_assets_changed(&state, existing.operasi_id, id, &auth.username).await;
    Ok(Json(ApiResponse { data: dto }))
}

/// 1-click restore original texture while keeping everything versioned for offline sync.
pub async fn restore_media_texture_original(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
) -> AppResult<Json<ApiResponse<MediaAssetDto>>> {
    let existing = fetch_media_by_id(&state.db, id).await?;
    let orig_size = existing
        .original_width
        .unwrap_or(0)
        .min(existing.original_height.unwrap_or(0)) as u32;
    resize_media_texture(
        State(state),
        auth,
        AxumPath(id),
        Json(ResizeTextureRequest {
            target_size: orig_size,
        }),
    )
    .await
}

pub async fn delete_media_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let existing = fetch_media_by_id(&state.db, id).await?;
    crate::auth::middleware::verify_operasi_asset_mutation(
        &state,
        &auth,
        existing.operasi_id,
        existing.created_by,
    )
    .await?;
    sqlx::query("DELETE FROM media_assets WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    let assets_dir = &state.config.assets_dir;
    for f in existing.all_files() {
        let _ = tokio::fs::remove_file(assets_dir.join(&f.rel_path)).await;
    }

    broadcast_media_assets_changed(&state, existing.operasi_id, id, &auth.username).await;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true, "id": id }),
    }))
}

struct ProcessedMedia {
    rel_path: String,
    sha256: String,
    size_bytes: i64,
    mime_type: String,
    width: Option<i32>,
    height: Option<i32>,
    duration_sec: Option<f64>,
    thumb_path: Option<String>,
    thumb_sha256: Option<String>,
    thumb_size_bytes: i64,
}

async fn process_and_store_media(
    assets_dir: &Path,
    operasi_id: Option<Uuid>,
    media_type: MediaAssetType,
    base_slug: &str,
    upload: &UploadedMediaFile,
    crop_mode: &str,
) -> AppResult<ProcessedMedia> {
    match media_type {
        MediaAssetType::Texture => {
            let bytes = upload.bytes.clone();
            let filename_lower = upload.filename.to_lowercase();
            let use_resize = crop_mode == "resize";

            let (encoded, side, is_png) =
                tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, u32, bool), String> {
                    let img = image::load_from_memory(&bytes).map_err(|e| {
                        format!("Format gambar texture tidak valid (gunakan PNG/JPG/WebP): {e}")
                    })?;
                    let w = img.width();
                    let h = img.height();
                    if w < 16 || h < 16 {
                        return Err(format!(
                            "Resolusi gambar terlalu kecil ({w}x{h}). Minimal 16x16 pixel."
                        ));
                    }

                    let side = w.min(h);
                    let square = if w == h {
                        img
                    } else if use_resize {
                        img.resize_exact(side, side, FilterType::Lanczos3)
                    } else {
                        // Default: Center-Crop to min(w, h) x min(w, h) without distortion
                        let x = (w - side) / 2;
                        let y = (h - side) / 2;
                        img.crop_imm(x, y, side, side)
                    };

                    let has_alpha = matches!(
                        square.color(),
                        ColorType::Rgba8
                            | ColorType::La8
                            | ColorType::Rgba16
                            | ColorType::La16
                            | ColorType::Rgba32F
                    );
                    let prefer_png = has_alpha
                        || filename_lower.ends_with(".png")
                        || filename_lower.ends_with(".webp");
                    let out = encode_texture_image(&square, prefer_png)?;
                    Ok((out, side, prefer_png))
                })
                .await
                .map_err(|e| AppError::Internal(e.into()))?
                .map_err(AppError::BadRequest)?;

            let ext = if is_png { "png" } else { "jpg" };
            let mime = if is_png { "image/png" } else { "image/jpeg" };
            let tex_dir = media_folder_prefix(operasi_id, "textures");
            let rel_path = format!("{tex_dir}/{base_slug}_orig.{ext}");
            let abs = assets_dir.join(&rel_path);
            if let Some(parent) = abs.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| {
                        format!("failed to create texture directory '{}'", parent.display())
                    })
                    .map_err(AppError::Internal)?;
            }
            tokio::fs::write(&abs, &encoded)
                .await
                .with_context(|| format!("failed to write texture asset '{}'", abs.display()))
                .map_err(AppError::Internal)?;

            Ok(ProcessedMedia {
                rel_path,
                sha256: sha256_hex(&encoded),
                size_bytes: encoded.len() as i64,
                mime_type: mime.to_string(),
                width: Some(side as i32),
                height: Some(side as i32),
                duration_sec: None,
                thumb_path: None,
                thumb_sha256: None,
                thumb_size_bytes: 0,
            })
        }
        MediaAssetType::Sound => {
            let ext = extract_allowed_ext(
                &upload.filename,
                &["mp3", "wav", "ogg", "flac", "aac", "m4a"],
                "Format audio tidak didukung. Gunakan .mp3, .wav, .ogg, .flac, .aac, atau .m4a",
            )?;
            let mime = mime_for_ext(&ext).to_string();
            let snd_dir = media_folder_prefix(operasi_id, "sounds");
            let rel_path = format!("{snd_dir}/{base_slug}.{ext}");
            let abs = assets_dir.join(&rel_path);
            if let Some(parent) = abs.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| {
                        format!("failed to create sound directory '{}'", parent.display())
                    })
                    .map_err(AppError::Internal)?;
            }
            tokio::fs::write(&abs, &upload.bytes)
                .await
                .with_context(|| format!("failed to write sound asset '{}'", abs.display()))
                .map_err(AppError::Internal)?;

            let probe = probe_media_with_ffprobe(&abs).await;

            Ok(ProcessedMedia {
                rel_path,
                sha256: sha256_hex(&upload.bytes),
                size_bytes: upload.bytes.len() as i64,
                mime_type: mime,
                width: None,
                height: None,
                duration_sec: probe.duration_sec,
                thumb_path: None,
                thumb_sha256: None,
                thumb_size_bytes: 0,
            })
        }
        MediaAssetType::Video => {
            let ext = extract_allowed_ext(
                &upload.filename,
                &["mp4", "webm", "mov", "mkv", "avi"],
                "Format video tidak didukung. Gunakan .mp4, .webm, .mov, .mkv, atau .avi",
            )?;
            let mime = mime_for_ext(&ext).to_string();
            let vid_dir = media_folder_prefix(operasi_id, "videos");
            let rel_path = format!("{vid_dir}/{base_slug}.{ext}");
            let abs = assets_dir.join(&rel_path);
            if let Some(parent) = abs.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| {
                        format!("failed to create video directory '{}'", parent.display())
                    })
                    .map_err(AppError::Internal)?;
            }
            tokio::fs::write(&abs, &upload.bytes)
                .await
                .with_context(|| format!("failed to write video asset '{}'", abs.display()))
                .map_err(AppError::Internal)?;

            let probe = probe_media_with_ffprobe(&abs).await;

            // Best-effort thumbnail generation via ffmpeg
            let thumb_rel = format!("{vid_dir}/thumbs/{base_slug}_thumb.jpg");
            let thumb_abs = assets_dir.join(&thumb_rel);
            let (thumb_path, thumb_sha256, thumb_size_bytes) =
                if generate_video_thumbnail(&abs, &thumb_abs).await {
                    if let Ok(tb) = tokio::fs::read(&thumb_abs).await
                        && !tb.is_empty()
                    {
                        (Some(thumb_rel), Some(sha256_hex(&tb)), tb.len() as i64)
                    } else {
                        (None, None, 0)
                    }
                } else {
                    (None, None, 0)
                };

            Ok(ProcessedMedia {
                rel_path,
                sha256: sha256_hex(&upload.bytes),
                size_bytes: upload.bytes.len() as i64,
                mime_type: mime,
                width: probe.width,
                height: probe.height,
                duration_sec: probe.duration_sec,
                thumb_path,
                thumb_sha256,
                thumb_size_bytes,
            })
        }
    }
}

fn encode_texture_image(img: &DynamicImage, as_png: bool) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    if as_png {
        img.write_to(&mut buf, ImageFormat::Png)
            .map_err(|e| format!("Gagal encode PNG: {e}"))?;
    } else {
        let rgb = img.to_rgb8();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 92);
        enc.encode_image(&rgb)
            .map_err(|e| format!("Gagal encode JPEG: {e}"))?;
    }
    Ok(buf.into_inner())
}

#[derive(Default)]
struct FfprobeInfo {
    width: Option<i32>,
    height: Option<i32>,
    duration_sec: Option<f64>,
}

async fn probe_media_with_ffprobe(path: &Path) -> FfprobeInfo {
    let Ok(out) = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .await
    else {
        return FfprobeInfo::default();
    };
    if !out.status.success() {
        return FfprobeInfo::default();
    }
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return FfprobeInfo::default();
    };

    let duration_sec = val
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d >= 0.0);

    let mut width = None;
    let mut height = None;
    if let Some(streams) = val.get("streams").and_then(|s| s.as_array()) {
        for st in streams {
            if st.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                width = st.get("width").and_then(|v| v.as_i64()).map(|n| n as i32);
                height = st.get("height").and_then(|v| v.as_i64()).map(|n| n as i32);
                break;
            }
        }
    }

    FfprobeInfo {
        width,
        height,
        duration_sec,
    }
}

async fn generate_video_thumbnail(video_path: &Path, thumb_out: &Path) -> bool {
    if let Some(parent) = thumb_out.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return false;
    }

    // Try grabbing a frame at 0.5s first, then fall back to 0.0s for very short videos
    for seek in ["00:00:00.5", "00:00:00"] {
        let Ok(out) = Command::new("ffmpeg")
            .args(["-y", "-ss", seek, "-i"])
            .arg(video_path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale='min(480,iw)':-2",
                "-q:v",
                "4",
            ])
            .arg(thumb_out)
            .output()
            .await
        else {
            return false;
        };
        if out.status.success() && thumb_out.exists() {
            return true;
        }
    }
    false
}

fn extract_allowed_ext(filename: &str, allowed: &[&str], err_msg: &str) -> AppResult<String> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if allowed.contains(&ext.as_str()) {
        Ok(ext)
    } else {
        Err(AppError::BadRequest(err_msg.into()))
    }
}

pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" | "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        _ => "application/octet-stream",
    }
}

async fn parse_media_multipart(mut multipart: Multipart) -> AppResult<ParsedMediaForm> {
    let mut fields = HashMap::new();
    let mut file = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart form: {e}")))?
    {
        let Some(name) = field.name().map(str::to_string) else {
            continue;
        };
        if let Some(fname) = field.file_name().map(str::to_string) {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("gagal membaca file '{name}': {e}")))?
                .to_vec();
            if !bytes.is_empty() {
                file = Some(UploadedMediaFile {
                    filename: fname,
                    bytes,
                });
            }
        } else {
            let text = field
                .text()
                .await
                .map_err(|e| AppError::BadRequest(format!("gagal membaca field '{name}': {e}")))?;
            fields.insert(name, text);
        }
    }

    Ok(ParsedMediaForm { fields, file })
}

fn sanitize_slug(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_sep = false;
        } else if !prev_sep && !out.is_empty() {
            out.push('_');
            prev_sep = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        format!("asset_{}", &Uuid::new_v4().simple().to_string()[..8])
    } else {
        trimmed
    }
}

async fn make_unique_media_base(
    db: &PgPool,
    slug_prefix: &str,
    base: &str,
    exclude_id: Option<Uuid>,
) -> AppResult<String> {
    let mut candidate = base.to_string();
    let mut suffix = 2u32;
    loop {
        let slug = format!("{slug_prefix}/{candidate}");
        let exists: i64 = match exclude_id {
            Some(eid) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE slug = $1 AND id <> $2")
                    .bind(&slug)
                    .bind(eid)
                    .fetch_one(db)
                    .await?
            }
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE slug = $1")
                    .bind(&slug)
                    .fetch_one(db)
                    .await?
            }
        };
        if exists == 0 {
            return Ok(candidate);
        }
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
}
