//! Idempotency keys: a retried money-moving request replays its stored response
//! instead of selling or charging twice. Callers run these inside their own transaction.
use crate::error::{AppError, AppResult};
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

const MIN_LEN: usize = 8;
const MAX_LEN: usize = 100;

pub enum Begin {
    Fresh,
    Replay(serde_json::Value),
}

/// Reads and validates the `Idempotency-Key` header (trimmed, 8-100 chars).
pub fn key_from_headers(h: &HeaderMap) -> AppResult<String> {
    let key = h
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|k| (MIN_LEN..=MAX_LEN).contains(&k.chars().count()))
        .ok_or_else(AppError::idempotency_key_required)?;
    Ok(key.to_owned())
}

/// Lowercase-hex SHA-256 of the JSON body's string form.
pub fn hash_body(v: &serde_json::Value) -> String {
    Sha256::digest(v.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Claims the key inside the caller's transaction. A concurrent holder blocks the insert
/// on the primary key until it commits (we then see its row) or rolls back (we win).
pub async fn begin(
    conn: &mut PgConnection,
    user: Uuid,
    scope: &str,
    key: &str,
    hash: &str,
) -> AppResult<Begin> {
    let inserted = sqlx::query!(
        "INSERT INTO idempotency_keys (user_id, scope, key, request_hash)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT DO NOTHING",
        user,
        scope,
        key,
        hash
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if inserted == 1 {
        return Ok(Begin::Fresh);
    }
    let row = sqlx::query!(
        "SELECT request_hash, response FROM idempotency_keys
         WHERE user_id = $1 AND scope = $2 AND key = $3",
        user,
        scope,
        key
    )
    .fetch_one(&mut *conn)
    .await?;
    if row.request_hash != hash {
        return Err(AppError::idempotency_conflict());
    }
    match row.response {
        Some(v) => Ok(Begin::Replay(v)),
        None => Err(AppError::Internal(anyhow::anyhow!(
            "idempotency key {key} exists without a stored response"
        ))),
    }
}

/// Stores the response so later retries with the same key replay it.
pub async fn finish(
    conn: &mut PgConnection,
    user: Uuid,
    scope: &str,
    key: &str,
    response: &serde_json::Value,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE idempotency_keys SET response = $4
         WHERE user_id = $1 AND scope = $2 AND key = $3",
        user,
        scope,
        key,
        response
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
