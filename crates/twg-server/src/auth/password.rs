use anyhow::{Result, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

pub fn hash_password(plain: &str) -> Result<String> {
    let hash = Argon2::default()
        .hash_password(plain.as_bytes())
        .map_err(|e| anyhow!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(plain: &str, phc_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_and_verify_roundtrip() {
        let hashed = hash_password("admin123").expect("hash should succeed");
        assert!(hashed.starts_with("$argon2"));
        assert!(verify_password("admin123", &hashed));
        assert!(!verify_password("wrong", &hashed));
    }
}
