//! Users, roles, sessions and authentication.
pub mod dto;
pub mod limiter;
pub mod password;
pub mod repo;
pub mod routes;
pub mod session;

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const MIN_PASSWORD_LEN: usize = 12;

/// The password length rule shared by admin and driver accounts.
pub fn check_password_length(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::Validation(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    Ok(())
}

/// Hash off the async executor (argon2 is slow on purpose).
pub async fn hash_password_blocking(password: &str) -> AppResult<String> {
    let pw = password.to_owned();
    Ok(
        tokio::task::spawn_blocking(move || password::hash_password(&pw))
            .await
            .map_err(|e| AppError::Internal(e.into()))??,
    )
}

/// Create an administrator account (used by the `create-admin` command).
pub async fn create_admin(
    pool: &PgPool,
    username: &str,
    display_name: &str,
    password: &str,
) -> AppResult<Uuid> {
    if username.trim().is_empty() {
        return Err(AppError::Validation("Username is required.".into()));
    }
    check_password_length(password)?;
    let hash = hash_password_blocking(password).await?;
    let mut tx = pool.begin().await?;
    let id = repo::create_user(&mut tx, username, display_name, &hash, &["admin"]).await?;
    tx.commit().await?;
    Ok(id)
}
