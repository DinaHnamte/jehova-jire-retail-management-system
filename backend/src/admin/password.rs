//! Argon2id password hashing (Architecture.md §13).
use std::sync::LazyLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Hash a password with Argon2id (default parameters) and a random salt; returns a PHC string.
pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(plain.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
}

/// Constant-time verify. Any malformed hash verifies false.
pub fn verify_password(plain: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

static DUMMY: LazyLock<String> = LazyLock::new(|| {
    hash_password("jj-dummy-password-for-unknown-users").expect("argon2 hashing works")
});

/// A real Argon2id hash used when the username doesn't exist, so unknown users
/// take as long to reject as wrong passwords. The login path never accepts it.
pub fn dummy_hash() -> &'static str {
    &DUMMY
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(verify_password("correct horse battery", &h));
    }

    #[test]
    fn verify_rejects_wrong_password() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(!verify_password("correct horse batterY", &h));
    }

    #[test]
    fn verify_with_dummy_hash_is_false() {
        assert!(!verify_password("", dummy_hash()));
        assert!(!verify_password("anything", dummy_hash()));
    }

    #[test]
    fn hash_is_argon2id_and_salted() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert!(a.starts_with("$argon2id$"));
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_hash_verifies_false() {
        assert!(!verify_password("x", "not-a-hash"));
    }
}
