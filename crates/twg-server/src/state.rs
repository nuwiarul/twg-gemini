use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::watch;

use crate::{auth::token::TokenService, config::AppConfig, ws::WsHub};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
    pub tokens: Arc<TokenService>,
    pub ws_hub: Arc<WsHub>,
    /// Broadcast when the HTTP server starts draining connections.
    pub shutdown: watch::Receiver<()>,
}
