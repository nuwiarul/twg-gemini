use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::{PgPool, Row};
use twg_shared::{
    api::{ApiResponse, UpsertSnapshotRequest},
    models::GameSessionSnapshot,
};
use uuid::Uuid;

use crate::{
    auth::middleware::{AuthUser, verify_skenario_access},
    error::AppResult,
    state::AppState,
};

pub async fn fetch_snapshot_optional(
    db: &PgPool,
    skenario_id: Uuid,
) -> Result<Option<GameSessionSnapshot>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT id, operasi_id, skenario_id, snapshot_enabled, map_state, markers_state, anims_state,
               version, updated_by, updated_at
        FROM game_session_snapshots
        WHERE skenario_id = $1
        "#,
    )
    .bind(skenario_id)
    .fetch_optional(db)
    .await?;

    row.as_ref().map(map_snapshot_row).transpose()
}

pub async fn get_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(skenario_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Option<GameSessionSnapshot>>>> {
    verify_skenario_access(&state, &auth, skenario_id).await?;
    let snap = fetch_snapshot_optional(&state.db, skenario_id).await?;
    Ok(Json(ApiResponse { data: snap }))
}

pub async fn upsert_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(skenario_id): Path<Uuid>,
    Json(req): Json<UpsertSnapshotRequest>,
) -> AppResult<Json<ApiResponse<GameSessionSnapshot>>> {
    auth.require_admin_write()?;
    verify_skenario_access(&state, &auth, skenario_id).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO game_session_snapshots (
            operasi_id, skenario_id, snapshot_enabled, map_state, markers_state, anims_state, version, updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, 1, $7)
        ON CONFLICT (skenario_id)
        DO UPDATE SET
            snapshot_enabled = EXCLUDED.snapshot_enabled,
            map_state = EXCLUDED.map_state,
            markers_state = EXCLUDED.markers_state,
            anims_state = EXCLUDED.anims_state,
            version = game_session_snapshots.version + 1,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        RETURNING id, operasi_id, skenario_id, snapshot_enabled, map_state, markers_state, anims_state,
                  version, updated_by, updated_at
        "#,
    )
    .bind(req.operasi_id)
    .bind(skenario_id)
    .bind(req.snapshot_enabled)
    .bind(req.map_state)
    .bind(req.markers_state)
    .bind(req.anims_state)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        data: map_snapshot_row(&row)?,
    }))
}

pub async fn delete_snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(skenario_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    auth.require_admin_write()?;
    verify_skenario_access(&state, &auth, skenario_id).await?;
    sqlx::query("DELETE FROM game_session_snapshots WHERE skenario_id = $1")
        .bind(skenario_id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse {
        data: serde_json::json!({ "deleted": true }),
    }))
}

fn map_snapshot_row(row: &sqlx::postgres::PgRow) -> Result<GameSessionSnapshot, sqlx::Error> {
    Ok(GameSessionSnapshot {
        id: row.try_get("id")?,
        operasi_id: row.try_get("operasi_id")?,
        skenario_id: row.try_get("skenario_id")?,
        snapshot_enabled: row.try_get("snapshot_enabled")?,
        map_state: row.try_get("map_state")?,
        markers_state: row.try_get("markers_state")?,
        anims_state: row.try_get("anims_state")?,
        version: row.try_get("version")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}
