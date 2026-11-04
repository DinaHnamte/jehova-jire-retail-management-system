use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    shared::ids::new_id,
};

pub struct LoginUser {
    pub id: Uuid,
    pub password_hash: String,
    pub is_active: bool,
}

/// Insert a user with the given roles. Run inside the caller's transaction.
pub async fn create_user(
    conn: &mut PgConnection,
    username: &str,
    display_name: &str,
    password_hash: &str,
    role_codes: &[&str],
) -> AppResult<Uuid> {
    let codes: Vec<String> = role_codes.iter().map(|s| s.to_string()).collect();
    let role_ids = sqlx::query_scalar!("SELECT id FROM roles WHERE code = ANY($1)", &codes)
        .fetch_all(&mut *conn)
        .await?;
    if role_ids.len() != codes.len() {
        return Err(AppError::Validation(format!("Unknown role in {codes:?}")));
    }

    let id = new_id();
    sqlx::query!(
        "INSERT INTO users (id, username, display_name, password_hash) VALUES ($1, $2, $3, $4)",
        id,
        username.trim(),
        display_name.trim(),
        password_hash
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| match e.as_database_error().and_then(|d| d.code()) {
        Some(code) if code == "23505" => {
            AppError::Conflict(format!("Username '{}' is already taken.", username.trim()))
        }
        _ => e.into(),
    })?;

    sqlx::query!(
        "INSERT INTO user_roles (user_id, role_id) SELECT $1, unnest($2::uuid[])",
        id,
        &role_ids
    )
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn find_user_for_login(
    conn: &mut PgConnection,
    username: &str,
) -> AppResult<Option<LoginUser>> {
    Ok(sqlx::query_as!(
        LoginUser,
        "SELECT id, password_hash, is_active FROM users WHERE lower(username) = lower($1)",
        username.trim()
    )
    .fetch_optional(&mut *conn)
    .await?)
}

pub async fn user_permissions(conn: &mut PgConnection, user_id: Uuid) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar!(
        "SELECT DISTINCT rp.permission_code FROM user_roles ur
         JOIN role_permissions rp ON rp.role_id = ur.role_id
         WHERE ur.user_id = $1 ORDER BY 1",
        user_id
    )
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn user_role_codes(conn: &mut PgConnection, user_id: Uuid) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar!(
        "SELECT r.code FROM user_roles ur JOIN roles r ON r.id = ur.role_id
         WHERE ur.user_id = $1 ORDER BY 1",
        user_id
    )
    .fetch_all(&mut *conn)
    .await?)
}
