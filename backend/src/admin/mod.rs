//! Users, roles, sessions and authentication.
pub mod password;
pub mod repo;

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const MIN_PASSWORD_LEN: usize = 12;

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
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::Validation(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    let pw = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || password::hash_password(&pw))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
    let mut tx = pool.begin().await?;
    let id = repo::create_user(&mut tx, username, display_name, &hash, &["admin"]).await?;
    tx.commit().await?;
    Ok(id)
}
