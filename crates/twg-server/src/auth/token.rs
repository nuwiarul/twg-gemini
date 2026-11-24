use core::convert::TryFrom;
use core::time::Duration;

use anyhow::{Result, anyhow};
use pasetors::{
    Local,
    claims::{Claims, ClaimsValidationRules},
    keys::SymmetricKey,
    local,
    token::UntrustedToken,
    version4::V4,
};
use sha2::{Digest, Sha256};
use twg_shared::models::UserRole;
use uuid::Uuid;

const IMPLICIT_ASSERTION: &[u8] = b"twg-rust-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
}

impl TokenKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenPayload {
    pub user_id: Uuid,
    pub username: String,
    pub role: UserRole,
    pub satker_id: Option<Uuid>,
    #[allow(dead_code)]
    pub kind: TokenKind,
}

pub struct TokenService {
    key: SymmetricKey<V4>,
    access_ttl_secs: u64,
    refresh_ttl_secs: u64,
}

impl TokenService {
    pub fn new(raw_key: &[u8; 32], access_ttl_secs: u64, refresh_ttl_secs: u64) -> Result<Self> {
        let key = SymmetricKey::<V4>::from(raw_key)
            .map_err(|e| anyhow!("invalid paseto symmetric key: {e}"))?;
        Ok(Self {
            key,
            access_ttl_secs,
            refresh_ttl_secs,
        })
    }

    pub fn issue_pair(
        &self,
        user_id: Uuid,
        username: &str,
        role: UserRole,
        satker_id: Option<Uuid>,
    ) -> Result<(String, String)> {
        let access = self.create_token(
            user_id,
            username,
            role,
            satker_id,
            TokenKind::Access,
            self.access_ttl_secs,
        )?;
        let refresh = self.create_token(
            user_id,
            username,
            role,
            satker_id,
            TokenKind::Refresh,
            self.refresh_ttl_secs,
        )?;
        Ok((access, refresh))
    }

    pub fn create_token(
        &self,
        user_id: Uuid,
        username: &str,
        role: UserRole,
        satker_id: Option<Uuid>,
        kind: TokenKind,
        ttl_secs: u64,
    ) -> Result<String> {
        let mut claims = Claims::new_expires_in(&Duration::from_secs(ttl_secs))
            .map_err(|e| anyhow!("claims init failed: {e}"))?;
        claims
            .add_additional("uid", user_id.to_string())
            .map_err(|e| anyhow!("claim uid failed: {e}"))?;
        claims
            .add_additional("username", username)
            .map_err(|e| anyhow!("claim username failed: {e}"))?;
        claims
            .add_additional("role", role.as_str())
            .map_err(|e| anyhow!("claim role failed: {e}"))?;
        claims
            .add_additional("kind", kind.as_str())
            .map_err(|e| anyhow!("claim kind failed: {e}"))?;
        if let Some(sid) = satker_id {
            claims
                .add_additional("satker_id", sid.to_string())
                .map_err(|e| anyhow!("claim satker_id failed: {e}"))?;
        }

        local::encrypt(&self.key, &claims, None, Some(IMPLICIT_ASSERTION))
            .map_err(|e| anyhow!("paseto encrypt failed: {e}"))
    }

    pub fn verify(&self, token_str: &str, expected_kind: TokenKind) -> Result<TokenPayload> {
        let rules = ClaimsValidationRules::new();
        let untrusted = UntrustedToken::<Local, V4>::try_from(token_str)
            .map_err(|e| anyhow!("malformed paseto token: {e}"))?;
        let trusted = local::decrypt(&self.key, &untrusted, &rules, None, Some(IMPLICIT_ASSERTION))
            .map_err(|e| anyhow!("invalid or expired token: {e}"))?;
        let claims = trusted
            .payload_claims()
            .ok_or_else(|| anyhow!("missing token claims"))?;

        let kind_str = claims
            .get_claim("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing token kind"))?;
        let actual_kind = match kind_str {
            "access" => TokenKind::Access,
            "refresh" => TokenKind::Refresh,
            _ => return Err(anyhow!("unknown token kind")),
        };
        if actual_kind != expected_kind {
            return Err(anyhow!("unexpected token kind"));
        }

        let uid_str = claims
            .get_claim("uid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing uid claim"))?;
        let user_id = Uuid::parse_str(uid_str)?;

        let username = claims
            .get_claim("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing username claim"))?
            .to_string();

        let role_str = claims
            .get_claim("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let role = UserRole::from_db(role_str);

        let satker_id = claims
            .get_claim("satker_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        Ok(TokenPayload {
            user_id,
            username,
            role,
            satker_id,
            kind: actual_kind,
        })
    }
}

pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paseto_v4_local_roundtrip_and_expiry() {
        let key = [7u8; 32];
        let svc = TokenService::new(&key, 3600, 7200).unwrap();
        let uid = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let (access, refresh) = svc
            .issue_pair(uid, "admin", UserRole::Admin, Some(sid))
            .unwrap();
        assert!(access.starts_with("v4.local."));
        assert!(refresh.starts_with("v4.local."));

        let decoded = svc.verify(&access, TokenKind::Access).unwrap();
        assert_eq!(decoded.user_id, uid);
        assert_eq!(decoded.username, "admin");
        assert_eq!(decoded.role, UserRole::Admin);
        assert_eq!(decoded.satker_id, Some(sid));

        // Refresh token rejected when access token is expected
        assert!(svc.verify(&refresh, TokenKind::Access).is_err());

        // Expired token (0s) rejected
        let expired = svc
            .create_token(uid, "admin", UserRole::Admin, None, TokenKind::Access, 0)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(svc.verify(&expired, TokenKind::Access).is_err());
    }
}
