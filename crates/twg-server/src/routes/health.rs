use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::{error::AppResult, state::AppState};

pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn readyz(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(&state.db).await?;
    Ok(Json(json!({ "status": "ready", "database": "ok" })))
}
