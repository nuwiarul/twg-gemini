use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path as AxumPath, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::process::Command;
use twg_shared::{
    api::{ApiResponse, CreateModelCategoryRequest, UpdateModelCategoryRequest},
    models::{
        ModelAssetDto, ModelAssetType, ModelCategoryCount, ModelCategoryDto, ModelFileMeta,
        ModelSyncManifest,
    },
};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthUser,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListModelAssetsQuery {
    pub category: Option<String>,
    pub model_type: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogSeedEntry {
    id: String,
    #[serde(default)]
    slug: String,
    name: String,
    category: String,
    #[serde(default)]
    model_type: ModelAssetType,
    model_path: String,
    #[serde(default)]
    model_sha256: Option<String>,
    #[serde(default)]
    model_size_bytes: i64,
    #[serde(default)]
    model_tris: i32,
    #[serde(default)]
    lod2_path: Option<String>,
    #[serde(default)]
    lod2_sha256: Option<String>,
    #[serde(default)]
    lod2_size_bytes: i64,
    #[serde(default)]
    lod2_tris: i32,
    #[serde(default)]
    walk_model_path: Option<String>,
    #[serde(default)]
    walk_sha256: Option<String>,
    #[serde(default)]
    walk_size_bytes: i64,
    #[serde(default)]
    walk_tris: i32,
    #[serde(default)]
    walk_lod2_path: Option<String>,
    #[serde(default)]
    walk_lod2_sha256: Option<String>,
    #[serde(default)]
    walk_lod2_size_bytes: i64,
    #[serde(default)]
    walk_lod2_tris: i32,
    #[serde(default = "default_height")]
    default_height: f64,
    #[serde(default)]
    default_rotation_deg: [f64; 3],
}

fn default_height() -> f64 {
    10.0
}

#[derive(Debug, Deserialize)]
struct ConverterFileStats {
    sha256: String,
    size_bytes: i64,
    tris: i32,
}

#[derive(Debug, Deserialize)]
struct ConverterResult {
    ok: bool,
    error: Option<String>,
    lod0: Option<ConverterFileStats>,
    lod2: Option<ConverterFileStats>,
}

struct UploadedFile {
    filename: String,
    bytes: Vec<u8>,
}

fn file_download_url(rel_path: &str) -> String {
    format!("/api/v1/assets/models/file/{}", rel_path.trim_start_matches('/'))
}

fn row_to_dto(row: &sqlx::postgres::PgRow) -> AppResult<ModelAssetDto> {
    let model_path: String = row.try_get("model_path")?;
    let model_sha256: String = row.try_get("model_sha256")?;
    let model_size_bytes: i64 = row.try_get("model_size_bytes")?;
    let model_tris: i32 = row.try_get("model_tris")?;

    let lod2_path: Option<String> = row.try_get("lod2_path")?;
    let lod2_sha256: Option<String> = row.try_get("lod2_sha256")?;
    let lod2_size_bytes: i64 = row.try_get("lod2_size_bytes")?;
    let lod2_tris: i32 = row.try_get("lod2_tris")?;

    let walk_path: Option<String> = row.try_get("walk_path")?;
    let walk_sha256: Option<String> = row.try_get("walk_sha256")?;
    let walk_size_bytes: i64 = row.try_get("walk_size_bytes")?;
    let walk_tris: i32 = row.try_get("walk_tris")?;

    let walk_lod2_path: Option<String> = row.try_get("walk_lod2_path")?;
    let walk_lod2_sha256: Option<String> = row.try_get("walk_lod2_sha256")?;
    let walk_lod2_size_bytes: i64 = row.try_get("walk_lod2_size_bytes")?;
    let walk_lod2_tris: i32 = row.try_get("walk_lod2_tris")?;

    let model_type_str: String = row.try_get("model_type")?;
    let default_rot_x: f64 = row.try_get("default_rot_x")?;
    let default_rot_y: f64 = row.try_get("default_rot_y")?;
    let default_rot_z: f64 = row.try_get("default_rot_z")?;

    let make_opt = |p: Option<String>, h: Option<String>, sz: i64, tris: i32| {
        match (p, h) {
            (Some(rel_path), Some(sha256)) if !rel_path.is_empty() && !sha256.is_empty() => {
                Some(ModelFileMeta {
                    download_url: file_download_url(&rel_path),
                    rel_path,
                    sha256,
                    size_bytes: sz,
                    tris,
                })
            }
            _ => None,
        }
    };

    Ok(ModelAssetDto {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        model_type: ModelAssetType::from_db(&model_type_str),
        model_file: ModelFileMeta {
            download_url: file_download_url(&model_path),
            rel_path: model_path,
            sha256: model_sha256,
            size_bytes: model_size_bytes,
            tris: model_tris,
        },
        lod2_file: make_opt(lod2_path, lod2_sha256, lod2_size_bytes, lod2_tris),
        walk_file: make_opt(walk_path, walk_sha256, walk_size_bytes, walk_tris),
        walk_lod2_file: make_opt(
            walk_lod2_path,
            walk_lod2_sha256,
            walk_lod2_size_bytes,
            walk_lod2_tris,
        ),
        default_height: row.try_get("default_height")?,
        default_rotation_deg: [default_rot_x, default_rot_y, default_rot_z],
        version: row.try_get("version")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

pub async fn fetch_all_models(db: &PgPool) -> AppResult<Vec<ModelAssetDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, slug, name, category, model_type,
            model_path, model_sha256, model_size_bytes, model_tris,
            lod2_path, lod2_sha256, lod2_size_bytes, lod2_tris,
            walk_path, walk_sha256, walk_size_bytes, walk_tris,
            walk_lod2_path, walk_lod2_sha256, walk_lod2_size_bytes, walk_lod2_tris,
            default_height, default_rot_x, default_rot_y, default_rot_z,
            version, created_at, updated_at
        FROM model_assets
        ORDER BY category ASC, name ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    rows.iter().map(row_to_dto).collect()
}

/// Seed PostgreSQL `model_assets` from `assets/catalog/models.json` when the table is empty.
pub async fn seed_model_assets_if_empty(db: &PgPool, assets_dir: &Path) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_assets")
        .fetch_one(db)
        .await?;
    if count > 0 {
        tracing::info!(count, "model_assets table already populated");
        return Ok(());
    }

    let catalog_path = assets_dir.join("catalog").join("models.json");
    if !catalog_path.exists() {
        tracing::warn!(
            path = %catalog_path.display(),
            "Catalog JSON not found; skipping initial model_assets import"
        );
        return Ok(());
    }

    let raw = tokio::fs::read_to_string(&catalog_path).await?;
    let entries: Vec<CatalogSeedEntry> = serde_json::from_str(&raw)?;
    if entries.is_empty() {
        return Ok(());
    }

    tracing::info!(
        total = entries.len(),
        "Importing converted 3D models from catalog JSON into PostgreSQL model_assets..."
    );

    let mut imported = 0usize;
    for entry in entries {
        let cat_slug = sanitize_category_slug(&entry.category);
        if cat_slug.is_empty() {
            tracing::warn!(name = %entry.name, category = %entry.category, "Skipping entry with invalid category");
            continue;
        }
        // Ensure category exists in model_categories
        sqlx::query(
            r#"
            INSERT INTO model_categories (slug, name, supports_idle_walk, default_scale, sort_order)
            VALUES ($1, $2, FALSE, 3.5, 100)
            ON CONFLICT (slug) DO NOTHING
            "#,
        )
        .bind(&cat_slug)
        .bind(&cat_slug)
        .execute(db)
        .await?;

        let primary_abs = assets_dir.join(&entry.model_path);
        if !primary_abs.exists() {
            tracing::warn!(path = %primary_abs.display(), "Skipping missing model file");
            continue;
        }

        let (model_sha256, model_size_bytes) = match entry.model_sha256 {
            Some(h) if !h.is_empty() && entry.model_size_bytes > 0 => (h, entry.model_size_bytes),
            _ => hash_and_size(&primary_abs).await?,
        };

        let (lod2_path, lod2_sha256, lod2_size_bytes) =
            resolve_optional_file(assets_dir, entry.lod2_path, entry.lod2_sha256, entry.lod2_size_bytes).await?;
        let (walk_path, walk_sha256, walk_size_bytes) =
            resolve_optional_file(assets_dir, entry.walk_model_path, entry.walk_sha256, entry.walk_size_bytes).await?;
        let (walk_lod2_path, walk_lod2_sha256, walk_lod2_size_bytes) =
            resolve_optional_file(assets_dir, entry.walk_lod2_path, entry.walk_lod2_sha256, entry.walk_lod2_size_bytes).await?;

        let id = Uuid::parse_str(&entry.id).unwrap_or_else(|_| Uuid::new_v4());
        let slug = if entry.slug.trim().is_empty() {
            format!("{}/{}", entry.category, sanitize_slug(&entry.name))
        } else {
            entry.slug
        };

        let model_type = if walk_path.is_some() {
            ModelAssetType::Animated
        } else {
            entry.model_type
        };

        sqlx::query(
            r#"
            INSERT INTO model_assets (
                id, slug, name, category, model_type,
                model_path, model_sha256, model_size_bytes, model_tris,
                lod2_path, lod2_sha256, lod2_size_bytes, lod2_tris,
                walk_path, walk_sha256, walk_size_bytes, walk_tris,
                walk_lod2_path, walk_lod2_sha256, walk_lod2_size_bytes, walk_lod2_tris,
                default_height, default_rot_x, default_rot_y, default_rot_z, version
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13,
                $14, $15, $16, $17,
                $18, $19, $20, $21,
                $22, $23, $24, $25, 1
            )
            ON CONFLICT (slug) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&slug)
        .bind(&entry.name)
        .bind(&entry.category)
        .bind(model_type.as_str())
        .bind(&entry.model_path)
        .bind(&model_sha256)
        .bind(model_size_bytes)
        .bind(entry.model_tris)
        .bind(&lod2_path)
        .bind(&lod2_sha256)
        .bind(lod2_size_bytes)
        .bind(entry.lod2_tris)
        .bind(&walk_path)
        .bind(&walk_sha256)
        .bind(walk_size_bytes)
        .bind(entry.walk_tris)
        .bind(&walk_lod2_path)
        .bind(&walk_lod2_sha256)
        .bind(walk_lod2_size_bytes)
        .bind(entry.walk_lod2_tris)
        .bind(entry.default_height)
        .bind(entry.default_rotation_deg[0])
        .bind(entry.default_rotation_deg[1])
        .bind(entry.default_rotation_deg[2])
        .execute(db)
        .await?;

        imported += 1;
    }

    tracing::info!(imported, "Imported 3D model assets into PostgreSQL");
    Ok(())
}

/// Idempotent startup repair: ensures every `.glb` file stored on the server has normalized
/// `WEIGHTS_0` skinning weights (`sum == 1.0`).
///
/// Whenever any `.glb` is repaired (or its on-disk SHA-256 differs from PostgreSQL), this updates
/// `model_assets` (`*_sha256`, `*_size_bytes`, `version = version + 1`) and rewrites
/// `assets/catalog/models.json`. All connected desktop clients (`AssetSyncBridge`) then detect the
/// new SHA-256 in `/api/v1/assets/models/manifest` and automatically download the clean `.glb`s.
pub async fn repair_unnormalized_skin_weights_if_needed(
    db: &PgPool,
    assets_dir: &Path,
) -> anyhow::Result<()> {
    let models = fetch_all_models(db)
        .await
        .map_err(|e| anyhow::anyhow!("failed loading model_assets for skin weight check: {e}"))?;
    if models.is_empty() {
        return Ok(());
    }

    // Collect every unique GLB path referenced in model_assets
    let mut unique_paths: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in &models {
        for f in m.all_files() {
            if f.rel_path.to_ascii_lowercase().ends_with(".glb")
                && seen.insert(f.rel_path.clone())
            {
                unique_paths.push(f.rel_path.clone());
            }
        }
    }

    let mut actual_meta: HashMap<String, (String, i64)> = HashMap::with_capacity(unique_paths.len());
    let mut repaired_files = 0usize;
    let mut repaired_vertices = 0usize;

    for rel_path in unique_paths {
        let abs = assets_dir.join(&rel_path);
        if !abs.exists() {
            continue;
        }
        let bytes = match tokio::fs::read(&abs).await {
            Ok(b) => b,
            Err(err) => {
                tracing::warn!(path = %abs.display(), ?err, "Skipping unreadable GLB during skin-weight check");
                continue;
            }
        };

        let final_bytes = match twg_shared::glb_skin::normalize_glb_joint_weights(&bytes) {
            Ok((Some(patched), report)) => {
                let tmp = PathBuf::from(format!("{}.norm.tmp", abs.display()));
                tokio::fs::write(&tmp, &patched).await?;
                if tokio::fs::rename(&tmp, &abs).await.is_err() {
                    tokio::fs::copy(&tmp, &abs).await?;
                    let _ = tokio::fs::remove_file(&tmp).await;
                }
                repaired_files += 1;
                repaired_vertices += report.repaired_vertices;
                patched
            }
            Ok((None, _)) => bytes,
            Err(err) => {
                tracing::warn!(path = %abs.display(), %err, "Skipping GLB weight normalization check");
                bytes
            }
        };

        let mut hasher = Sha256::new();
        hasher.update(&final_bytes);
        let sha = hex::encode(hasher.finalize());
        actual_meta.insert(rel_path, (sha, final_bytes.len() as i64));
    }

    // Update any DB rows whose stored SHA-256 or file size no longer matches disk
    let mut updated_rows = 0usize;
    for m in &models {
        let resolve_slot = |f: &ModelFileMeta| -> (String, i64, bool) {
            if let Some((sha, sz)) = actual_meta.get(&f.rel_path) {
                let changed = sha != &f.sha256 || *sz != f.size_bytes;
                (sha.clone(), *sz, changed)
            } else {
                (f.sha256.clone(), f.size_bytes, false)
            }
        };
        let resolve_opt_slot = |opt: Option<&ModelFileMeta>| -> (Option<String>, i64, bool) {
            match opt {
                Some(f) => {
                    let (sha, sz, ch) = resolve_slot(f);
                    (Some(sha), sz, ch)
                }
                None => (None, 0, false),
            }
        };

        let (m_sha, m_sz, m_ch) = resolve_slot(&m.model_file);
        let (l2_sha, l2_sz, l2_ch) = resolve_opt_slot(m.lod2_file.as_ref());
        let (w_sha, w_sz, w_ch) = resolve_opt_slot(m.walk_file.as_ref());
        let (wl2_sha, wl2_sz, wl2_ch) = resolve_opt_slot(m.walk_lod2_file.as_ref());

        if !(m_ch || l2_ch || w_ch || wl2_ch) {
            continue;
        }

        sqlx::query(
            r#"
            UPDATE model_assets
            SET
                model_sha256 = $2,
                model_size_bytes = $3,
                lod2_sha256 = $4,
                lod2_size_bytes = $5,
                walk_sha256 = $6,
                walk_size_bytes = $7,
                walk_lod2_sha256 = $8,
                walk_lod2_size_bytes = $9,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(m.id)
        .bind(&m_sha)
        .bind(m_sz)
        .bind(&l2_sha)
        .bind(l2_sz)
        .bind(&w_sha)
        .bind(w_sz)
        .bind(&wl2_sha)
        .bind(wl2_sz)
        .execute(db)
        .await?;

        updated_rows += 1;
    }

    if updated_rows > 0 {
        let _ = write_catalog_snapshot(db, assets_dir).await;
        tracing::info!(
            repaired_files,
            repaired_vertices,
            updated_rows,
            "Normalized unnormalized GLB skinning weights on server and bumped sync manifest hashes"
        );
    } else if repaired_files > 0 {
        tracing::info!(
            repaired_files,
            repaired_vertices,
            "Normalized unnormalized GLB skinning weights on disk"
        );
    } else {
        tracing::info!("All server GLB skinning weights verified normalized");
    }

    Ok(())
}

async fn resolve_optional_file(
    assets_dir: &Path,
    path_opt: Option<String>,
    sha_opt: Option<String>,
    size: i64,
) -> anyhow::Result<(Option<String>, Option<String>, i64)> {
    let Some(rel) = path_opt.filter(|s| !s.is_empty()) else {
        return Ok((None, None, 0));
    };
    let abs = assets_dir.join(&rel);
    if !abs.exists() {
        return Ok((None, None, 0));
    }
    match sha_opt {
        Some(h) if !h.is_empty() && size > 0 => Ok((Some(rel), Some(h), size)),
        _ => {
            let (h, sz) = hash_and_size(&abs).await?;
            Ok((Some(rel), Some(h), sz))
        }
    }
}

async fn hash_and_size(path: &Path) -> anyhow::Result<(String, i64)> {
    let bytes = tokio::fs::read(path).await?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = hex::encode(hasher.finalize());
    Ok((sha, bytes.len() as i64))
}

async fn fetch_all_categories(db: &PgPool) -> AppResult<Vec<ModelCategoryDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.slug,
            c.name,
            c.supports_idle_walk,
            c.default_scale,
            c.sort_order,
            c.created_at,
            c.updated_at,
            COUNT(m.id)::BIGINT AS total_models,
            COUNT(m.id) FILTER (WHERE m.model_type = 'static')::BIGINT AS static_count,
            COUNT(m.id) FILTER (WHERE m.model_type = 'animated')::BIGINT AS animated_count
        FROM model_categories c
        LEFT JOIN model_assets m ON m.category = c.slug
        GROUP BY c.slug, c.name, c.supports_idle_walk, c.default_scale, c.sort_order, c.created_at, c.updated_at
        ORDER BY c.sort_order ASC, c.slug ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(ModelCategoryDto {
            slug: row.try_get("slug")?,
            name: row.try_get("name")?,
            supports_idle_walk: row.try_get("supports_idle_walk")?,
            default_scale: row.try_get("default_scale")?,
            sort_order: row.try_get("sort_order")?,
            total_models: row.try_get::<i64, _>("total_models")?.max(0) as usize,
            static_count: row.try_get::<i64, _>("static_count")?.max(0) as usize,
            animated_count: row.try_get::<i64, _>("animated_count")?.max(0) as usize,
            created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
            updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
        });
    }
    Ok(out)
}

async fn fetch_category_by_slug(db: &PgPool, slug: &str) -> AppResult<ModelCategoryDto> {
    let row = sqlx::query(
        r#"
        SELECT
            c.slug,
            c.name,
            c.supports_idle_walk,
            c.default_scale,
            c.sort_order,
            c.created_at,
            c.updated_at,
            COUNT(m.id)::BIGINT AS total_models,
            COUNT(m.id) FILTER (WHERE m.model_type = 'static')::BIGINT AS static_count,
            COUNT(m.id) FILTER (WHERE m.model_type = 'animated')::BIGINT AS animated_count
        FROM model_categories c
        LEFT JOIN model_assets m ON m.category = c.slug
        WHERE c.slug = $1
        GROUP BY c.slug, c.name, c.supports_idle_walk, c.default_scale, c.sort_order, c.created_at, c.updated_at
        "#,
    )
    .bind(slug)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Kategori '{slug}' tidak ditemukan")))?;

    Ok(ModelCategoryDto {
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        supports_idle_walk: row.try_get("supports_idle_walk")?,
        default_scale: row.try_get("default_scale")?,
        sort_order: row.try_get("sort_order")?,
        total_models: row.try_get::<i64, _>("total_models")?.max(0) as usize,
        static_count: row.try_get::<i64, _>("static_count")?.max(0) as usize,
        animated_count: row.try_get::<i64, _>("animated_count")?.max(0) as usize,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

async fn category_exists(db: &PgPool, slug: &str) -> AppResult<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_categories WHERE slug = $1)")
            .bind(slug)
            .fetch_one(db)
            .await?;
    Ok(exists)
}

fn sanitize_category_slug(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_sep = false;
        } else if (ch == '_' || ch == '-' || ch == ' ') && !out.is_empty() && !prev_sep {
            out.push(if ch == '-' { '-' } else { '_' });
            prev_sep = true;
        }
    }
    let trimmed = out.trim_matches(|c| c == '_' || c == '-').to_string();
    if trimmed.len() > 63 {
        trimmed[..63].trim_matches(|c| c == '_' || c == '-').to_string()
    } else {
        trimmed
    }
}

pub async fn list_model_categories(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<Vec<ModelCategoryDto>>>> {
    let items = fetch_all_categories(&state.db).await?;
    Ok(Json(ApiResponse { data: items }))
}

pub async fn create_model_category(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateModelCategoryRequest>,
) -> AppResult<Json<ApiResponse<ModelCategoryDto>>> {
    auth.require_super_admin_write()?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Nama kategori wajib diisi".into()));
    }

    let raw_slug = if req.slug.trim().is_empty() {
        &name
    } else {
        req.slug.as_str()
    };
    let slug = sanitize_category_slug(raw_slug);
    if slug.is_empty() || slug == "all" {
        return Err(AppError::BadRequest(
            "Slug kategori tidak valid (gunakan huruf kecil, angka, '_' atau '-', bukan 'all')".into(),
        ));
    }

    let scale = if req.default_scale.is_finite() && req.default_scale > 0.0 {
        req.default_scale.clamp(0.01, 100.0)
    } else {
        0.1
    };

    sqlx::query(
        r#"
        INSERT INTO model_categories (slug, name, supports_idle_walk, default_scale, sort_order)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&slug)
    .bind(&name)
    .bind(req.supports_idle_walk)
    .bind(scale)
    .bind(req.sort_order)
    .execute(&state.db)
    .await?;

    // Create category folder on server disk immediately
    let cat_dir = state.config.assets_dir.join("models").join(&slug);
    let _ = tokio::fs::create_dir_all(&cat_dir).await;

    let dto = fetch_category_by_slug(&state.db, &slug).await?;
    Ok(Json(ApiResponse { data: dto }))
}

pub async fn update_model_category(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(slug): AxumPath<String>,
    Json(req): Json<UpdateModelCategoryRequest>,
) -> AppResult<Json<ApiResponse<ModelCategoryDto>>> {
    auth.require_super_admin_write()?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Nama kategori tidak boleh kosong".into()));
    }
    let scale = if req.default_scale.is_finite() && req.default_scale > 0.0 {
        req.default_scale.clamp(0.01, 100.0)
    } else {
        0.1
    };

    let res = sqlx::query(
        r#"
        UPDATE model_categories
        SET name = $2, supports_idle_walk = $3, default_scale = $4, sort_order = $5, updated_at = NOW()
        WHERE slug = $1
        "#,
    )
    .bind(&slug)
    .bind(&name)
    .bind(req.supports_idle_walk)
    .bind(scale)
    .bind(req.sort_order)
    .execute(&state.db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Kategori '{slug}' tidak ditemukan")));
    }

    let dto = fetch_category_by_slug(&state.db, &slug).await?;
    Ok(Json(ApiResponse { data: dto }))
}

pub async fn delete_model_category(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(slug): AxumPath<String>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_super_admin_write()?;

    if !category_exists(&state.db, &slug).await? {
        return Err(AppError::NotFound(format!("Kategori '{slug}' tidak ditemukan")));
    }

    let model_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM model_assets WHERE category = $1")
            .bind(&slug)
            .fetch_one(&state.db)
            .await?;

    if model_count > 0 {
        return Err(AppError::Conflict(format!(
            "Kategori '{slug}' masih memiliki {model_count} model. Pindahkan atau hapus model terlebih dahulu."
        )));
    }

    sqlx::query("DELETE FROM model_categories WHERE slug = $1")
        .bind(&slug)
        .execute(&state.db)
        .await?;

    // Remove directory only if it is completely empty
    let cat_dir = state.config.assets_dir.join("models").join(&slug);
    let _ = tokio::fs::remove_dir(&cat_dir).await;

    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true, "slug": slug }),
    }))
}

pub async fn get_sync_manifest(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<ModelSyncManifest>>> {
    let models = fetch_all_models(&state.db).await?;
    let cat_dtos = fetch_all_categories(&state.db).await?;
    let media_assets = crate::routes::media_assets::fetch_all_media(&state.db).await?;

    let mut total_files = 0usize;
    let mut total_bytes = 0u64;

    let mut manifest_hasher = Sha256::new();
    for c in &cat_dtos {
        manifest_hasher.update(c.slug.as_bytes());
        manifest_hasher.update(c.name.as_bytes());
        manifest_hasher.update([c.supports_idle_walk as u8]);
        manifest_hasher.update(c.sort_order.to_le_bytes());
    }
    for m in &models {
        manifest_hasher.update(m.id.as_bytes());
        manifest_hasher.update(m.version.to_le_bytes());
        manifest_hasher.update(m.updated_at.timestamp_millis().to_le_bytes());
        for f in m.all_files() {
            total_files += 1;
            total_bytes += f.size_bytes.max(0) as u64;
            manifest_hasher.update(f.rel_path.as_bytes());
            manifest_hasher.update(f.sha256.as_bytes());
        }
    }
    for ma in &media_assets {
        manifest_hasher.update(ma.id.as_bytes());
        manifest_hasher.update(ma.version.to_le_bytes());
        manifest_hasher.update(ma.updated_at.timestamp_millis().to_le_bytes());
        for f in ma.all_files() {
            total_files += 1;
            total_bytes += f.size_bytes.max(0) as u64;
            manifest_hasher.update(f.rel_path.as_bytes());
            manifest_hasher.update(f.sha256.as_bytes());
        }
    }

    let categories: Vec<ModelCategoryCount> = cat_dtos
        .iter()
        .map(ModelCategoryCount::from_dto)
        .collect();

    let manifest = ModelSyncManifest {
        manifest_hash: hex::encode(manifest_hasher.finalize()),
        generated_at: Utc::now(),
        total_models: models.len(),
        total_media: media_assets.len(),
        total_files,
        total_bytes,
        categories,
        models,
        media_assets,
    };

    Ok(Json(ApiResponse { data: manifest }))
}

pub async fn list_model_assets(
    State(state): State<AppState>,
    Query(q): Query<ListModelAssetsQuery>,
) -> AppResult<Json<ApiResponse<Vec<ModelAssetDto>>>> {
    let mut models = fetch_all_models(&state.db).await?;
    if let Some(cat) = q.category.as_deref().filter(|s| !s.is_empty() && *s != "all") {
        models.retain(|m| m.category == cat);
    }
    if let Some(mt) = q.model_type.as_deref().filter(|s| !s.is_empty() && *s != "all") {
        models.retain(|m| m.model_type.as_str() == mt);
    }
    if let Some(search) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let needle = search.to_lowercase();
        models.retain(|m| {
            m.name.to_lowercase().contains(&needle)
                || m.slug.to_lowercase().contains(&needle)
                || m.category.to_lowercase().contains(&needle)
        });
    }
    Ok(Json(ApiResponse { data: models }))
}

pub async fn get_model_asset(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> AppResult<Json<ApiResponse<ModelAssetDto>>> {
    let dto = fetch_model_by_id(&state.db, id).await?;
    Ok(Json(ApiResponse { data: dto }))
}

async fn fetch_model_by_id(db: &PgPool, id: Uuid) -> AppResult<ModelAssetDto> {
    let row = sqlx::query(
        r#"
        SELECT
            id, slug, name, category, model_type,
            model_path, model_sha256, model_size_bytes, model_tris,
            lod2_path, lod2_sha256, lod2_size_bytes, lod2_tris,
            walk_path, walk_sha256, walk_size_bytes, walk_tris,
            walk_lod2_path, walk_lod2_sha256, walk_lod2_size_bytes, walk_lod2_tris,
            default_height, default_rot_x, default_rot_y, default_rot_z,
            version, created_at, updated_at
        FROM model_assets
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("model asset not found".into()))?;

    row_to_dto(&row)
}

pub async fn download_model_file(
    State(state): State<AppState>,
    AxumPath(rel_path): AxumPath<String>,
) -> AppResult<Response> {
    let clean = sanitize_relative_asset_path(&rel_path)?;
    if !clean.starts_with("models/") && !clean.starts_with("media/") {
        return Err(AppError::Forbidden(
            "only models/* or media/* files can be downloaded".into(),
        ));
    }

    let abs = state.config.assets_dir.join(&clean);
    let bytes = tokio::fs::read(&abs)
        .await
        .map_err(|_| AppError::NotFound(format!("asset file not found: {clean}")))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let etag = format!("\"{}\"", hex::encode(hasher.finalize()));

    let ext = Path::new(&clean)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let content_type = crate::routes::media_assets::mime_for_ext(ext);

    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    if let Ok(v) = HeaderValue::from_str(&etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    Ok(resp)
}

pub async fn create_model_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> AppResult<Json<ApiResponse<ModelAssetDto>>> {
    auth.require_super_admin_write()?;

    let form = parse_model_multipart(multipart).await?;
    let name = form
        .fields
        .get("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Nama model wajib diisi".into()))?;

    let category = form
        .fields
        .get("category")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Kategori wajib dipilih".into()))?;
    if !category_exists(&state.db, &category).await? {
        return Err(AppError::BadRequest(format!(
            "Kategori '{category}' belum terdaftar. Tambahkan kategori terlebih dahulu di Kelola Kategori."
        )));
    }

    let model_type_raw = form
        .fields
        .get("model_type")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "static".to_string());
    let model_type = match model_type_raw.as_str() {
        "static" | "statis" => ModelAssetType::Static,
        "animated" | "animasi" => ModelAssetType::Animated,
        _ => {
            return Err(AppError::BadRequest(
                "model_type harus 'static' atau 'animated'".into(),
            ));
        }
    };

    let default_height = parse_f64(&form.fields, "default_height", 10.0);
    let default_rot_x = parse_f64(&form.fields, "default_rot_x", 0.0);
    let default_rot_y = parse_f64(&form.fields, "default_rot_y", 0.0);
    let default_rot_z = parse_f64(&form.fields, "default_rot_z", 0.0);

    let base_slug = sanitize_slug(&name);
    let unique_base = make_unique_base_slug(&state.db, &category, &base_slug, None).await?;
    let slug = format!("{category}/{unique_base}");

    let assets_dir = &state.config.assets_dir;
    let id = Uuid::new_v4();

    let (
        model_path,
        model_lod0,
        lod2_path,
        model_lod2,
        walk_path,
        walk_lod0,
        walk_lod2_path,
        walk_lod2,
    ) = match model_type {
        ModelAssetType::Static => {
            let file = form
                .files
                .get("model_file")
                .or_else(|| form.files.get("idle_file"))
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "File model 3D (.glb/.gltf) wajib diunggah untuk tipe Statis".into(),
                    )
                })?;
            validate_gltf_extension(&file.filename)?;

            let rel_lod0 = format!("models/{category}/{unique_base}.glb");
            let rel_lod2 = format!("models/{category}/{unique_base}_lod2.glb");
            let (s0, s2) = convert_uploaded_model(
                file,
                &assets_dir.join(&rel_lod0),
                &assets_dir.join(&rel_lod2),
                "any",
            )
            .await?;
            (rel_lod0, s0, Some(rel_lod2), Some(s2), None, None, None, None)
        }
        ModelAssetType::Animated => {
            let idle_file = form.files.get("idle_file").ok_or_else(|| {
                AppError::BadRequest(
                    "Model Animasi wajib mengunggah 2 file: file Idle (*_idle.glb/.gltf tanpa animasi) dan file Walk (*_walk.glb/.gltf dengan animasi jalan)".into(),
                )
            })?;
            let walk_file = form.files.get("walk_file").ok_or_else(|| {
                AppError::BadRequest(
                    "Model Animasi wajib mengunggah 2 file: file Walk (*_walk.glb/.gltf dengan animasi jalan) belum dipilih".into(),
                )
            })?;

            validate_gltf_extension(&idle_file.filename)?;
            validate_gltf_extension(&walk_file.filename)?;

            let rel_idle = format!("models/{category}/{unique_base}_idle.glb");
            let rel_idle_lod2 = format!("models/{category}/{unique_base}_idle_lod2.glb");
            let rel_walk = format!("models/{category}/{unique_base}_walk.glb");
            let rel_walk_lod2 = format!("models/{category}/{unique_base}_walk_lod2.glb");

            let (i0, i2) = convert_uploaded_model(
                idle_file,
                &assets_dir.join(&rel_idle),
                &assets_dir.join(&rel_idle_lod2),
                "none",
            )
            .await?;

            let (w0, w2) = match convert_uploaded_model(
                walk_file,
                &assets_dir.join(&rel_walk),
                &assets_dir.join(&rel_walk_lod2),
                "required",
            )
            .await
            {
                Ok(res) => res,
                Err(err) => {
                    // Roll back idle files on walk validation failure
                    let _ = tokio::fs::remove_file(assets_dir.join(&rel_idle)).await;
                    let _ = tokio::fs::remove_file(assets_dir.join(&rel_idle_lod2)).await;
                    return Err(err);
                }
            };

            (
                rel_idle,
                i0,
                Some(rel_idle_lod2),
                Some(i2),
                Some(rel_walk),
                Some(w0),
                Some(rel_walk_lod2),
                Some(w2),
            )
        }
    };

    sqlx::query(
        r#"
        INSERT INTO model_assets (
            id, slug, name, category, model_type,
            model_path, model_sha256, model_size_bytes, model_tris,
            lod2_path, lod2_sha256, lod2_size_bytes, lod2_tris,
            walk_path, walk_sha256, walk_size_bytes, walk_tris,
            walk_lod2_path, walk_lod2_sha256, walk_lod2_size_bytes, walk_lod2_tris,
            default_height, default_rot_x, default_rot_y, default_rot_z,
            version, created_by
        )
        VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9,
            $10, $11, $12, $13,
            $14, $15, $16, $17,
            $18, $19, $20, $21,
            $22, $23, $24, $25,
            1, $26
        )
        "#,
    )
    .bind(id)
    .bind(&slug)
    .bind(&name)
    .bind(&category)
    .bind(model_type.as_str())
    .bind(&model_path)
    .bind(&model_lod0.sha256)
    .bind(model_lod0.size_bytes)
    .bind(model_lod0.tris)
    .bind(&lod2_path)
    .bind(model_lod2.as_ref().map(|s| s.sha256.as_str()))
    .bind(model_lod2.as_ref().map(|s| s.size_bytes).unwrap_or(0))
    .bind(model_lod2.as_ref().map(|s| s.tris).unwrap_or(0))
    .bind(&walk_path)
    .bind(walk_lod0.as_ref().map(|s| s.sha256.as_str()))
    .bind(walk_lod0.as_ref().map(|s| s.size_bytes).unwrap_or(0))
    .bind(walk_lod0.as_ref().map(|s| s.tris).unwrap_or(0))
    .bind(&walk_lod2_path)
    .bind(walk_lod2.as_ref().map(|s| s.sha256.as_str()))
    .bind(walk_lod2.as_ref().map(|s| s.size_bytes).unwrap_or(0))
    .bind(walk_lod2.as_ref().map(|s| s.tris).unwrap_or(0))
    .bind(default_height)
    .bind(default_rot_x)
    .bind(default_rot_y)
    .bind(default_rot_z)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    let dto = fetch_model_by_id(&state.db, id).await?;
    let _ = write_catalog_snapshot(&state.db, assets_dir).await;
    Ok(Json(ApiResponse { data: dto }))
}

pub async fn update_model_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
    multipart: Multipart,
) -> AppResult<Json<ApiResponse<ModelAssetDto>>> {
    auth.require_super_admin_write()?;

    let existing = fetch_model_by_id(&state.db, id).await?;
    let form = parse_model_multipart(multipart).await?;

    let name = form
        .fields
        .get("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| existing.name.clone());

    let category = match form.fields.get("category").map(|s| s.trim()) {
        Some(c) if !c.is_empty() => {
            if !category_exists(&state.db, c).await? {
                return Err(AppError::BadRequest(format!(
                    "Kategori '{c}' belum terdaftar. Tambahkan kategori terlebih dahulu di Kelola Kategori."
                )));
            }
            c.to_string()
        }
        _ => existing.category.clone(),
    };

    let model_type = match form.fields.get("model_type").map(|s| s.trim().to_lowercase()) {
        Some(t) if t == "static" || t == "statis" => ModelAssetType::Static,
        Some(t) if t == "animated" || t == "animasi" => ModelAssetType::Animated,
        Some(t) if !t.is_empty() => {
            return Err(AppError::BadRequest(
                "model_type harus 'static' atau 'animated'".into(),
            ));
        }
        _ => existing.model_type,
    };

    let default_height = parse_f64(&form.fields, "default_height", existing.default_height);
    let default_rot_x = parse_f64(
        &form.fields,
        "default_rot_x",
        existing.default_rotation_deg[0],
    );
    let default_rot_y = parse_f64(
        &form.fields,
        "default_rot_y",
        existing.default_rotation_deg[1],
    );
    let default_rot_z = parse_f64(
        &form.fields,
        "default_rot_z",
        existing.default_rotation_deg[2],
    );

    let assets_dir = &state.config.assets_dir;

    // Extract current base name from existing slug
    let old_base = existing
        .slug
        .split('/')
        .nth(1)
        .unwrap_or("model")
        .to_string();
    let new_base = if category != existing.category {
        make_unique_base_slug(&state.db, &category, &old_base, Some(id)).await?
    } else {
        old_base
    };
    let new_slug = format!("{category}/{new_base}");

    let mut model_path = existing.model_file.rel_path.clone();
    let mut model_sha = existing.model_file.sha256.clone();
    let mut model_size = existing.model_file.size_bytes;
    let mut model_tris = existing.model_file.tris;

    let mut lod2_path = existing.lod2_file.as_ref().map(|f| f.rel_path.clone());
    let mut lod2_sha = existing.lod2_file.as_ref().map(|f| f.sha256.clone());
    let mut lod2_size = existing.lod2_file.as_ref().map(|f| f.size_bytes).unwrap_or(0);
    let mut lod2_tris = existing.lod2_file.as_ref().map(|f| f.tris).unwrap_or(0);

    let mut walk_path = existing.walk_file.as_ref().map(|f| f.rel_path.clone());
    let mut walk_sha = existing.walk_file.as_ref().map(|f| f.sha256.clone());
    let mut walk_size = existing.walk_file.as_ref().map(|f| f.size_bytes).unwrap_or(0);
    let mut walk_tris = existing.walk_file.as_ref().map(|f| f.tris).unwrap_or(0);

    let mut walk_lod2_path = existing.walk_lod2_file.as_ref().map(|f| f.rel_path.clone());
    let mut walk_lod2_sha = existing.walk_lod2_file.as_ref().map(|f| f.sha256.clone());
    let mut walk_lod2_size = existing
        .walk_lod2_file
        .as_ref()
        .map(|f| f.size_bytes)
        .unwrap_or(0);
    let mut walk_lod2_tris = existing.walk_lod2_file.as_ref().map(|f| f.tris).unwrap_or(0);

    let mut files_to_cleanup_after: Vec<String> = Vec::new();

    // If category changed, relocate kept files to the new category directory first
    if category != existing.category {
        model_path = relocate_file_to_category(assets_dir, &model_path, &category).await?;
        if let Some(p) = &lod2_path {
            lod2_path = Some(relocate_file_to_category(assets_dir, p, &category).await?);
        }
        if let Some(p) = &walk_path {
            walk_path = Some(relocate_file_to_category(assets_dir, p, &category).await?);
        }
        if let Some(p) = &walk_lod2_path {
            walk_lod2_path = Some(relocate_file_to_category(assets_dir, p, &category).await?);
        }
    }

    match model_type {
        ModelAssetType::Static => {
            if let Some(file) = form.files.get("model_file").or_else(|| form.files.get("idle_file")) {
                validate_gltf_extension(&file.filename)?;
                let target_lod0 = format!("models/{category}/{new_base}.glb");
                let target_lod2 = format!("models/{category}/{new_base}_lod2.glb");
                let (s0, s2) = convert_uploaded_model(
                    file,
                    &assets_dir.join(&target_lod0),
                    &assets_dir.join(&target_lod2),
                    "any",
                )
                .await?;
                if model_path != target_lod0 {
                    files_to_cleanup_after.push(model_path);
                }
                if let Some(old_l2) = &lod2_path
                    && old_l2 != &target_lod2
                {
                    files_to_cleanup_after.push(old_l2.clone());
                }
                model_path = target_lod0;
                model_sha = s0.sha256;
                model_size = s0.size_bytes;
                model_tris = s0.tris;
                lod2_path = Some(target_lod2);
                lod2_sha = Some(s2.sha256);
                lod2_size = s2.size_bytes;
                lod2_tris = s2.tris;
            }
            // If converting from animated -> static, clear walk files
            if let Some(wp) = walk_path.take() {
                files_to_cleanup_after.push(wp);
            }
            if let Some(wlp) = walk_lod2_path.take() {
                files_to_cleanup_after.push(wlp);
            }
            walk_sha = None;
            walk_size = 0;
            walk_tris = 0;
            walk_lod2_sha = None;
            walk_lod2_size = 0;
            walk_lod2_tris = 0;
        }
        ModelAssetType::Animated => {
            let switching_from_static = existing.model_type == ModelAssetType::Static;
            let new_idle = form.files.get("idle_file");
            let new_walk = form.files.get("walk_file");

            if switching_from_static && (new_idle.is_none() || new_walk.is_none()) {
                return Err(AppError::BadRequest(
                    "Mengubah tipe model ke Animasi wajib mengunggah kedua file: Idle (*_idle tanpa animasi) dan Walk (*_walk dengan animasi jalan)".into(),
                ));
            }

            if let Some(idle_file) = new_idle {
                validate_gltf_extension(&idle_file.filename)?;
                let target_idle = format!("models/{category}/{new_base}_idle.glb");
                let target_idle_lod2 = format!("models/{category}/{new_base}_idle_lod2.glb");
                let (i0, i2) = convert_uploaded_model(
                    idle_file,
                    &assets_dir.join(&target_idle),
                    &assets_dir.join(&target_idle_lod2),
                    "none",
                )
                .await?;
                if model_path != target_idle {
                    files_to_cleanup_after.push(model_path);
                }
                if let Some(old_l2) = &lod2_path
                    && old_l2 != &target_idle_lod2
                {
                    files_to_cleanup_after.push(old_l2.clone());
                }
                model_path = target_idle;
                model_sha = i0.sha256;
                model_size = i0.size_bytes;
                model_tris = i0.tris;
                lod2_path = Some(target_idle_lod2);
                lod2_sha = Some(i2.sha256);
                lod2_size = i2.size_bytes;
                lod2_tris = i2.tris;
            }

            if let Some(walk_file) = new_walk {
                validate_gltf_extension(&walk_file.filename)?;
                let target_walk = format!("models/{category}/{new_base}_walk.glb");
                let target_walk_lod2 = format!("models/{category}/{new_base}_walk_lod2.glb");
                let (w0, w2) = convert_uploaded_model(
                    walk_file,
                    &assets_dir.join(&target_walk),
                    &assets_dir.join(&target_walk_lod2),
                    "required",
                )
                .await?;
                if let Some(old_w) = &walk_path
                    && old_w != &target_walk
                {
                    files_to_cleanup_after.push(old_w.clone());
                }
                if let Some(old_wl2) = &walk_lod2_path
                    && old_wl2 != &target_walk_lod2
                {
                    files_to_cleanup_after.push(old_wl2.clone());
                }
                walk_path = Some(target_walk);
                walk_sha = Some(w0.sha256);
                walk_size = w0.size_bytes;
                walk_tris = w0.tris;
                walk_lod2_path = Some(target_walk_lod2);
                walk_lod2_sha = Some(w2.sha256);
                walk_lod2_size = w2.size_bytes;
                walk_lod2_tris = w2.tris;
            }

            if walk_path.is_none() {
                return Err(AppError::BadRequest(
                    "Model Animasi harus memiliki file Walk".into(),
                ));
            }
        }
    }

    sqlx::query(
        r#"
        UPDATE model_assets
        SET
            slug = $2,
            name = $3,
            category = $4,
            model_type = $5,
            model_path = $6,
            model_sha256 = $7,
            model_size_bytes = $8,
            model_tris = $9,
            lod2_path = $10,
            lod2_sha256 = $11,
            lod2_size_bytes = $12,
            lod2_tris = $13,
            walk_path = $14,
            walk_sha256 = $15,
            walk_size_bytes = $16,
            walk_tris = $17,
            walk_lod2_path = $18,
            walk_lod2_sha256 = $19,
            walk_lod2_size_bytes = $20,
            walk_lod2_tris = $21,
            default_height = $22,
            default_rot_x = $23,
            default_rot_y = $24,
            default_rot_z = $25,
            version = version + 1,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&new_slug)
    .bind(&name)
    .bind(&category)
    .bind(model_type.as_str())
    .bind(&model_path)
    .bind(&model_sha)
    .bind(model_size)
    .bind(model_tris)
    .bind(&lod2_path)
    .bind(&lod2_sha)
    .bind(lod2_size)
    .bind(lod2_tris)
    .bind(&walk_path)
    .bind(&walk_sha)
    .bind(walk_size)
    .bind(walk_tris)
    .bind(&walk_lod2_path)
    .bind(&walk_lod2_sha)
    .bind(walk_lod2_size)
    .bind(walk_lod2_tris)
    .bind(default_height)
    .bind(default_rot_x)
    .bind(default_rot_y)
    .bind(default_rot_z)
    .execute(&state.db)
    .await?;

    for old_rel in files_to_cleanup_after {
        remove_file_if_unreferenced(&state.db, assets_dir, &old_rel).await;
    }

    let dto = fetch_model_by_id(&state.db, id).await?;
    let _ = write_catalog_snapshot(&state.db, assets_dir).await;
    Ok(Json(ApiResponse { data: dto }))
}

pub async fn delete_model_asset(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_super_admin_write()?;

    let existing = fetch_model_by_id(&state.db, id).await?;
    sqlx::query("DELETE FROM model_assets WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    let assets_dir = &state.config.assets_dir;
    for f in existing.all_files() {
        remove_file_if_unreferenced(&state.db, assets_dir, &f.rel_path).await;
    }

    let _ = write_catalog_snapshot(&state.db, assets_dir).await;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true, "id": id }),
    }))
}

async fn remove_file_if_unreferenced(db: &PgPool, assets_dir: &Path, rel_path: &str) {
    let refs: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM model_assets
        WHERE model_path = $1 OR lod2_path = $1 OR walk_path = $1 OR walk_lod2_path = $1
        "#,
    )
    .bind(rel_path)
    .fetch_one(db)
    .await
    .unwrap_or(1);

    if refs == 0 {
        let _ = tokio::fs::remove_file(assets_dir.join(rel_path)).await;
    }
}

async fn relocate_file_to_category(
    assets_dir: &Path,
    old_rel: &str,
    new_category: &str,
) -> AppResult<String> {
    let fname = Path::new(old_rel)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invalid model filename: {old_rel}")))?;
    let new_rel = format!("models/{new_category}/{fname}");
    if new_rel == old_rel {
        return Ok(new_rel);
    }
    let old_abs = assets_dir.join(old_rel);
    let new_abs = assets_dir.join(&new_rel);
    if let Some(parent) = new_abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    if old_abs.exists() {
        if let Err(_) = tokio::fs::rename(&old_abs, &new_abs).await {
            tokio::fs::copy(&old_abs, &new_abs)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            let _ = tokio::fs::remove_file(&old_abs).await;
        }
    }
    Ok(new_rel)
}

async fn write_catalog_snapshot(db: &PgPool, assets_dir: &Path) -> anyhow::Result<()> {
    let models = fetch_all_models(db)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let snapshot: Vec<serde_json::Value> = models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id.to_string(),
                "slug": m.slug,
                "name": m.name,
                "category": m.category,
                "model_type": m.model_type.as_str(),
                "model_path": m.model_file.rel_path,
                "model_sha256": m.model_file.sha256,
                "model_size_bytes": m.model_file.size_bytes,
                "model_tris": m.model_file.tris,
                "lod2_path": m.lod2_file.as_ref().map(|f| &f.rel_path),
                "lod2_sha256": m.lod2_file.as_ref().map(|f| &f.sha256),
                "lod2_size_bytes": m.lod2_file.as_ref().map(|f| f.size_bytes).unwrap_or(0),
                "lod2_tris": m.lod2_file.as_ref().map(|f| f.tris).unwrap_or(0),
                "walk_model_path": m.walk_file.as_ref().map(|f| &f.rel_path),
                "walk_sha256": m.walk_file.as_ref().map(|f| &f.sha256),
                "walk_size_bytes": m.walk_file.as_ref().map(|f| f.size_bytes).unwrap_or(0),
                "walk_tris": m.walk_file.as_ref().map(|f| f.tris).unwrap_or(0),
                "walk_lod2_path": m.walk_lod2_file.as_ref().map(|f| &f.rel_path),
                "walk_lod2_sha256": m.walk_lod2_file.as_ref().map(|f| &f.sha256),
                "walk_lod2_size_bytes": m.walk_lod2_file.as_ref().map(|f| f.size_bytes).unwrap_or(0),
                "walk_lod2_tris": m.walk_lod2_file.as_ref().map(|f| f.tris).unwrap_or(0),
                "default_height": m.default_height,
                "default_rotation_deg": m.default_rotation_deg,
                "ready": true,
            })
        })
        .collect();

    let catalog_path = assets_dir.join("catalog").join("models.json");
    if let Some(parent) = catalog_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = catalog_path.with_extension("json.tmp");
    tokio::fs::write(&tmp, serde_json::to_string_pretty(&snapshot)?).await?;
    tokio::fs::rename(&tmp, &catalog_path).await?;
    Ok(())
}

struct ParsedModelForm {
    fields: HashMap<String, String>,
    files: HashMap<String, UploadedFile>,
}

async fn parse_model_multipart(mut multipart: Multipart) -> AppResult<ParsedModelForm> {
    let mut fields = HashMap::new();
    let mut files = HashMap::new();

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
                .map_err(|e| AppError::BadRequest(format!("failed reading file '{name}': {e}")))?
                .to_vec();
            if !bytes.is_empty() {
                files.insert(
                    name,
                    UploadedFile {
                        filename: fname,
                        bytes,
                    },
                );
            }
        } else {
            let text = field
                .text()
                .await
                .map_err(|e| AppError::BadRequest(format!("failed reading field '{name}': {e}")))?;
            fields.insert(name, text);
        }
    }

    Ok(ParsedModelForm { fields, files })
}

fn validate_gltf_extension(filename: &str) -> AppResult<()> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".glb") || lower.ends_with(".gltf") {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Format file tidak didukung ('{filename}'). Gunakan file .glb atau .gltf"
        )))
    }
}

fn parse_f64(fields: &HashMap<String, String>, key: &str, default: f64) -> f64 {
    fields
        .get(key)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

fn sanitize_slug(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        format!("model_{}", &Uuid::new_v4().simple().to_string()[..8])
    } else {
        trimmed
    }
}

async fn make_unique_base_slug(
    db: &PgPool,
    category: &str,
    base: &str,
    exclude_id: Option<Uuid>,
) -> AppResult<String> {
    let mut candidate = base.to_string();
    let mut suffix = 2u32;
    loop {
        let slug = format!("{category}/{candidate}");
        let exists: i64 = match exclude_id {
            Some(eid) => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM model_assets WHERE slug = $1 AND id <> $2",
                )
                .bind(&slug)
                .bind(eid)
                .fetch_one(db)
                .await?
            }
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM model_assets WHERE slug = $1")
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

fn sanitize_relative_asset_path(raw: &str) -> AppResult<String> {
    let normalized = raw.trim().trim_start_matches('/').replace('\\', "/");
    let p = Path::new(&normalized);
    let mut parts = Vec::new();
    for comp in p.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg
                    .to_str()
                    .ok_or_else(|| AppError::BadRequest("invalid UTF-8 path".into()))?;
                parts.push(s);
            }
            _ => {
                return Err(AppError::BadRequest(
                    "invalid path traversal component".into(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(AppError::BadRequest("empty file path".into()));
    }
    Ok(parts.join("/"))
}

fn find_converter_script() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("scripts/convert-single-model.mjs"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/convert-single-model.mjs"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

async fn convert_uploaded_model(
    upload: &UploadedFile,
    lod0_out: &Path,
    lod2_out: &Path,
    expect_anim_mode: &str,
) -> AppResult<(ConverterFileStats, ConverterFileStats)> {
    let ext = if upload.filename.to_lowercase().ends_with(".gltf") {
        "gltf"
    } else {
        "glb"
    };

    let tmp_dir = std::env::temp_dir().join("twg-model-convert");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let tmp_in = tmp_dir.join(format!("{}.{ext}", Uuid::new_v4()));
    tokio::fs::write(&tmp_in, &upload.bytes)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let tmp_lod0 = tmp_dir.join(format!("{}_lod0.glb", Uuid::new_v4()));
    let tmp_lod2 = tmp_dir.join(format!("{}_lod2.glb", Uuid::new_v4()));

    let result = if let Some(script_path) = find_converter_script() {
        let output = Command::new("node")
            .arg(&script_path)
            .arg("--input")
            .arg(&tmp_in)
            .arg("--lod0")
            .arg(&tmp_lod0)
            .arg("--lod2")
            .arg(&tmp_lod2)
            .arg("--anim")
            .arg(expect_anim_mode)
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let parsed: ConverterResult = serde_json::from_str(stdout.trim()).map_err(|e| {
                    AppError::BadRequest(format!(
                        "Gagal memproses model 3D: {} {}",
                        stderr.trim(),
                        e
                    ))
                })?;
                if !parsed.ok {
                    Err(AppError::BadRequest(
                        parsed
                            .error
                            .unwrap_or_else(|| "Konversi model 3D gagal".into()),
                    ))
                } else {
                    let s0 = parsed
                        .lod0
                        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing lod0 stats")))?;
                    let s2 = parsed
                        .lod2
                        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing lod2 stats")))?;
                    Ok((s0, s2))
                }
            }
            Err(err) => {
                tracing::warn!(?err, "Node converter unavailable; using pure-Rust GLB fallback");
                rust_glb_fallback(&upload.bytes, &tmp_lod0, &tmp_lod2, expect_anim_mode).await
            }
        }
    } else {
        rust_glb_fallback(&upload.bytes, &tmp_lod0, &tmp_lod2, expect_anim_mode).await
    };

    let _ = tokio::fs::remove_file(&tmp_in).await;

    match result {
        Ok((mut s0, mut s2)) => {
            // Final pure-Rust safety net: guarantee both LOD0 and LOD2 have exact unit WEIGHTS_0
            // before publishing to assets_dir & PostgreSQL (covers both Node and Rust paths).
            ensure_glb_file_normalized(&tmp_lod0, &mut s0).await?;
            ensure_glb_file_normalized(&tmp_lod2, &mut s2).await?;

            if let Some(p) = lod0_out.parent() {
                tokio::fs::create_dir_all(p)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
            }
            move_file(&tmp_lod0, lod0_out).await?;
            move_file(&tmp_lod2, lod2_out).await?;
            Ok((s0, s2))
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&tmp_lod0).await;
            let _ = tokio::fs::remove_file(&tmp_lod2).await;
            Err(err)
        }
    }
}

async fn ensure_glb_file_normalized(
    path: &Path,
    stats: &mut ConverterFileStats,
) -> AppResult<()> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let (patched_opt, report) = twg_shared::glb_skin::normalize_glb_joint_weights(&bytes)
        .map_err(|e| AppError::BadRequest(format!("GLB skinning data tidak valid: {e}")))?;
    if let Some(patched) = patched_opt {
        tokio::fs::write(path, &patched)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut hasher = Sha256::new();
        hasher.update(&patched);
        stats.sha256 = hex::encode(hasher.finalize());
        stats.size_bytes = patched.len() as i64;
        tracing::info!(
            path = %path.display(),
            repaired_vertices = report.repaired_vertices,
            "Normalized unnormalized skinning WEIGHTS_0 on uploaded GLB"
        );
    }
    Ok(())
}

async fn move_file(src: &Path, dst: &Path) -> AppResult<()> {
    if let Err(_) = tokio::fs::rename(src, dst).await {
        tokio::fs::copy(src, dst)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let _ = tokio::fs::remove_file(src).await;
    }
    Ok(())
}

/// Pure-Rust fallback validation if Node converter isn't present on the host.
async fn rust_glb_fallback(
    bytes: &[u8],
    lod0_out: &Path,
    lod2_out: &Path,
    expect_anim_mode: &str,
) -> AppResult<(ConverterFileStats, ConverterFileStats)> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(AppError::BadRequest(
            "File harus berupa binary GLB valid (atau aktifkan Node gltf-transform untuk .gltf)".into(),
        ));
    }
    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if 20 + json_len > bytes.len() {
        return Err(AppError::BadRequest("Header JSON GLB rusak".into()));
    }
    let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len])
        .map_err(|e| AppError::BadRequest(format!("JSON GLB tidak valid: {e}")))?;

    for key in ["extensionsRequired", "extensionsUsed"] {
        if let Some(arr) = json.get(key).and_then(|v| v.as_array()) {
            for ext in arr {
                if ext.as_str() == Some("KHR_draco_mesh_compression") {
                    return Err(AppError::BadRequest(
                        "Model menggunakan kompresi Draco; Node gltf-transform diperlukan untuk konversi Bevy".into(),
                    ));
                }
            }
        }
    }

    let anim_count = json
        .get("animations")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if expect_anim_mode == "none" && anim_count > 0 {
        return Err(AppError::BadRequest(format!(
            "Model idle tidak boleh memiliki animasi, ditemukan {anim_count} animasi"
        )));
    }
    if expect_anim_mode == "required" && anim_count == 0 {
        return Err(AppError::BadRequest(
            "Model walk wajib memiliki animasi jalan, ditemukan 0 animasi".into(),
        ));
    }

    let (patched_opt, report) = twg_shared::glb_skin::normalize_glb_joint_weights(bytes)
        .map_err(|e| AppError::BadRequest(format!("GLB skinning data tidak valid: {e}")))?;
    let final_bytes: &[u8] = patched_opt.as_deref().unwrap_or(bytes);
    if report.repaired_vertices > 0 {
        tracing::info!(
            repaired_vertices = report.repaired_vertices,
            "Normalized unnormalized skinning WEIGHTS_0 in uploaded GLB (Rust fallback)"
        );
    }

    tokio::fs::write(lod0_out, final_bytes)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tokio::fs::write(lod2_out, final_bytes)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut hasher = Sha256::new();
    hasher.update(final_bytes);
    let sha = hex::encode(hasher.finalize());
    let stats = ConverterFileStats {
        sha256: sha,
        size_bytes: final_bytes.len() as i64,
        tris: 0,
    };
    Ok((
        ConverterFileStats {
            sha256: stats.sha256.clone(),
            size_bytes: stats.size_bytes,
            tris: stats.tris,
        },
        stats,
    ))
}
