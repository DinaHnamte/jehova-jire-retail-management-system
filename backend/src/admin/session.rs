//! Server-side sessions (Architecture.md §13). The browser holds a random token in an
//! HttpOnly cookie; the database holds only its SHA-256.
use std::collections::HashSet;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    admin::repo,
    error::{AppError, AppResult},
    shared::ids::new_id,
};

pub const COOKIE_NAME: &str = "jj_session";
pub const STAFF_IDLE_SECS: i32 = 12 * 3600;
pub const DRIVER_IDLE_SECS: i32 = 30 * 86400;
/// Cookie lifetime; the server enforces the (shorter) idle timeout.
pub const COOKIE_MAX_AGE_SECS: i32 = DRIVER_IDLE_SECS;

/// The logged-in user for this request. Extract it in a handler to require login.
#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub permissions: HashSet<String>,
    pub session_id: Uuid,
    pub csrf_token: String,
}

impl CurrentUser {
    pub fn can(&self, perm: &str) -> bool {
        self.permissions.contains(perm)
    }

    pub fn require(&self, perm: &str) -> AppResult<()> {
        if self.can(perm) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or(AppError::Unauthenticated)
    }
}

/// Idle timeout for a user with these roles: 30 days only if they are purely a driver.
pub fn idle_secs_for_roles(roles: &[String]) -> i32 {
    if !roles.is_empty() && roles.iter().all(|r| r == "delivery_driver") {
        DRIVER_IDLE_SECS
    } else {
        STAFF_IDLE_SECS
    }
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random number generator available");
    URL_SAFE_NO_PAD.encode(buf)
}

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub struct NewSession {
    pub token: String,
    pub csrf_token: String,
}

pub async fn create(
    conn: &mut PgConnection,
    user_id: Uuid,
    idle_secs: i32,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> AppResult<NewSession> {
    let token = random_token();
    let csrf_token = random_token();
    sqlx::query!(
        "INSERT INTO sessions (id, token_hash, csrf_token, user_id, expires_at, idle_timeout_secs, user_agent, ip)
         VALUES ($1, $2, $3, $4, now() + $5::int * interval '1 second', $5, $6, $7)",
        new_id(),
        hash_token(&token),
        csrf_token,
        user_id,
        idle_secs,
        user_agent,
        ip
    )
    .execute(conn)
    .await?;
    Ok(NewSession { token, csrf_token })
}

/// Resolve a cookie token to a user. Expired sessions and inactive users resolve to None.
/// Slides the idle expiry forward at most every 5 minutes.
pub async fn lookup(conn: &mut PgConnection, token: &str) -> AppResult<Option<CurrentUser>> {
    let Some(row) = sqlx::query!(
        "SELECT s.id AS session_id, s.csrf_token, u.id AS user_id, u.username, u.display_name
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now() AND u.is_active",
        hash_token(token)
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    sqlx::query!(
        "UPDATE sessions SET last_seen_at = now(), expires_at = now() + idle_timeout_secs * interval '1 second'
         WHERE id = $1 AND last_seen_at < now() - interval '5 minutes'",
        row.session_id
    )
    .execute(&mut *conn)
    .await?;

    let permissions = repo::user_permissions(conn, row.user_id)
        .await?
        .into_iter()
        .collect();
    Ok(Some(CurrentUser {
        id: row.user_id,
        username: row.username,
        display_name: row.display_name,
        permissions,
        session_id: row.session_id,
        csrf_token: row.csrf_token,
    }))
}

pub async fn delete(conn: &mut PgConnection, session_id: Uuid) -> AppResult<()> {
    sqlx::query!("DELETE FROM sessions WHERE id = $1", session_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_all_for_user(conn: &mut PgConnection, user_id: Uuid) -> AppResult<()> {
    sqlx::query!("DELETE FROM sessions WHERE user_id = $1", user_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// The session token from the request's cookies, if any.
pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

pub fn set_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={COOKIE_MAX_AGE_SECS}{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_only_gets_long_sessions() {
        let r = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            idle_secs_for_roles(&r(&["delivery_driver"])),
            DRIVER_IDLE_SECS
        );
        assert_eq!(
            idle_secs_for_roles(&r(&["delivery_driver", "cashier"])),
            STAFF_IDLE_SECS
        );
        assert_eq!(idle_secs_for_roles(&r(&[])), STAFF_IDLE_SECS);
    }

    #[test]
    fn tokens_are_random_and_long() {
        let (a, b) = (random_token(), random_token());
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn finds_session_cookie_among_others() {
        let mut h = HeaderMap::new();
        h.insert("cookie", "a=1; jj_session=tok; b=2".parse().unwrap());
        assert_eq!(token_from_headers(&h).as_deref(), Some("tok"));
        h.insert("cookie", "jj_session_x=no".parse().unwrap());
        assert_eq!(token_from_headers(&h), None);
    }
}
