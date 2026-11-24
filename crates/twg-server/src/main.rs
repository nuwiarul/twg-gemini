mod auth;
mod config;
mod error;
mod routes;
mod seed;
mod state;
mod ws;

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    auth::token::TokenService, config::AppConfig, routes::build_router, state::AppState, ws::WsHub,
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,twg_server=debug,sqlx=warn")),
        )
        .init();

    let config = Arc::new(AppConfig::from_env()?);
    tokio::fs::create_dir_all(&config.upload_dir).await?;

    tracing::info!(addr = %config.bind_addr, "Connecting to PostgreSQL...");
    let db = PgPoolOptions::new()
        .max_connections(16)
        .connect(&config.database_url)
        .await?;

    tracing::info!("Running SQLx migrations...");
    sqlx::migrate!("./migrations").run(&db).await?;

    seed::run_seed_if_empty(&db).await?;
    routes::model_assets::seed_model_assets_if_empty(&db, &config.assets_dir).await?;

    let tokens = Arc::new(TokenService::new(
        &config.paseto_key,
        config.access_token_ttl_secs,
        config.refresh_token_ttl_secs,
    )?);
    let ws_hub = Arc::new(WsHub::new());

    let state = AppState {
        db,
        config: Arc::clone(&config),
        tokens,
        ws_hub,
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("twg-server listening on http://{}", config.bind_addr);

    axum::serve(listener, app).await?;
    Ok(())
}
