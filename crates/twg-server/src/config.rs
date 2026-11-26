use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub paseto_key: [u8; 32],
    pub access_token_ttl_secs: u64,
    pub refresh_token_ttl_secs: u64,
    pub upload_dir: PathBuf,
    pub assets_dir: PathBuf,
    #[allow(dead_code)]
    pub cors_allowed_origins: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://twg:twg_dev_password@127.0.0.1:5435/twg_rust".into());
        let bind_addr = std::env::var("SERVER_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8010".into());

        let key_raw = std::env::var("PASETO_SYMMETRIC_KEY").unwrap_or_else(|_| {
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()
        });
        let paseto_key = parse_32_byte_key(&key_raw)
            .context("PASETO_SYMMETRIC_KEY must be 64 hex chars or 32 ASCII bytes")?;

        let access_token_ttl_secs = std::env::var("ACCESS_TOKEN_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(86_400);
        let refresh_token_ttl_secs = std::env::var("REFRESH_TOKEN_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(604_800);

        let upload_dir =
            PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "assets/uploads".into()));
        let assets_dir =
            PathBuf::from(std::env::var("ASSETS_DIR").unwrap_or_else(|_| "assets".into()));
        let cors_allowed_origins =
            std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| "*".into());

        Ok(Self {
            database_url,
            bind_addr,
            paseto_key,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
            upload_dir,
            assets_dir,
            cors_allowed_origins,
        })
    }
}

fn parse_32_byte_key(raw: &str) -> Result<[u8; 32]> {
    let trimmed = raw.trim();
    if trimmed.len() == 64
        && let Ok(decoded) = hex::decode(trimmed)
        && decoded.len() == 32
    {
        let mut out = [0u8; 32];
        out.copy_from_slice(&decoded);
        return Ok(out);
    }
    if trimmed.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(trimmed.as_bytes());
        return Ok(out);
    }
    anyhow::bail!("expected 64-character hex string or 32-byte key");
}
