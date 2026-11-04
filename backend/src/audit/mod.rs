//! Audit log for security and privileged business actions (Architecture.md §13).
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{error::AppResult, http::request_id, shared::ids::new_id};

pub struct Entry<'a> {
    pub actor_id: Option<Uuid>,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<Uuid>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Write one audit row. Never pass passwords, tokens or secrets in `before`/`after`.
pub async fn record(conn: &mut PgConnection, e: Entry<'_>) -> AppResult<()> {
    sqlx::query!(
        "INSERT INTO audit_logs (id, actor_id, action, entity_type, entity_id, before, after, request_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        new_id(),
        e.actor_id,
        e.action,
        e.entity_type,
        e.entity_id,
        e.before,
        e.after,
        request_id::current()
    )
    .execute(conn)
    .await?;
    Ok(())
}
