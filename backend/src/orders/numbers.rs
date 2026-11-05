//! Gap-free document numbers per type and fiscal year (spec §3).
use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use super::domain::{fiscal_year, format_number};
use crate::error::AppResult;

/// Takes the next number for `doc_type` in the fiscal year of `at`, e.g. `JJ/2026-27/000001`.
/// The sequence row stays locked until the caller's transaction ends, so concurrent sales
/// queue here and a rolled-back sale gives its number back.
pub async fn next_number(
    conn: &mut PgConnection,
    doc_type: &str,
    at: DateTime<Utc>,
) -> AppResult<String> {
    let fy = fiscal_year(at);
    let seq = sqlx::query_scalar!(
        r#"INSERT INTO document_sequences (doc_type, fiscal_year, next_value) VALUES ($1, $2, 2)
           ON CONFLICT (doc_type, fiscal_year)
           DO UPDATE SET next_value = document_sequences.next_value + 1
           RETURNING next_value - 1 AS "seq!""#,
        doc_type,
        fy
    )
    .fetch_one(conn)
    .await?;
    Ok(format_number(&fy, seq))
}
