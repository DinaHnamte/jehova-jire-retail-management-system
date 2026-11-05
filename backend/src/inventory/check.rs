//! Nightly stock check (spec §2 rule 7): compare each balance with the sum of its movements.
//! Reports mismatches; never corrects them.
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{SHOP_LOCATION_ID, ist};
use crate::{AppState, admin::session::CurrentUser, error::AppResult, shared::ids::new_id};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Mismatch {
    pub product_id: Uuid,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub name: String,
    pub balance: i64,
    pub ledger_sum: i64,
}

#[derive(Debug, Serialize)]
pub struct CheckRun {
    pub id: Uuid,
    pub ran_at: DateTime<Utc>,
    pub mismatches: Vec<Mismatch>,
}

/// The next 02:00 shop time strictly after `now`.
pub fn next_run_after(now: DateTime<Utc>) -> DateTime<Utc> {
    let two_am = NaiveTime::from_hms_opt(2, 0, 0).expect("valid time");
    let local = now.with_timezone(&ist());
    let mut day = local.date_naive();
    loop {
        let candidate = day
            .and_time(two_am)
            .and_local_timezone(ist())
            .single()
            .expect("fixed offset is unambiguous")
            .with_timezone(&Utc);
        if candidate > now {
            return candidate;
        }
        day += Duration::days(1);
    }
}

/// Compare balances with ledger sums, store the run, and log any mismatch.
pub async fn run(db: &PgPool) -> AppResult<CheckRun> {
    let rows = sqlx::query!(
        r#"SELECT product_id AS "product_id!",
                  COALESCE(b.on_hand, 0) AS "balance!",
                  COALESCE(s.total, 0)::bigint AS "ledger_sum!"
           FROM (SELECT product_id, sum(quantity) AS total FROM stock_movements
                  WHERE location_id = $1 GROUP BY product_id) s
           FULL JOIN (SELECT product_id, on_hand FROM stock_balances WHERE location_id = $1) b
             USING (product_id)
           WHERE COALESCE(b.on_hand, 0) <> COALESCE(s.total, 0)
           ORDER BY product_id"#,
        SHOP_LOCATION_ID
    )
    .fetch_all(db)
    .await?;
    let stored: Vec<Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "product_id": r.product_id, "balance": r.balance, "ledger_sum": r.ledger_sum
            })
        })
        .collect();
    let id = new_id();
    sqlx::query!(
        "INSERT INTO stock_check_runs (id, mismatches) VALUES ($1, $2)",
        id,
        Value::Array(stored)
    )
    .execute(db)
    .await?;
    if !rows.is_empty() {
        tracing::error!(
            mismatches = rows.len(),
            "stock check found balances that differ from the ledger"
        );
    }
    load(db, id).await.map(|r| r.expect("just inserted"))
}

async fn load(db: &PgPool, id: Uuid) -> AppResult<Option<CheckRun>> {
    let Some(row) = sqlx::query!(
        "SELECT id, ran_at, mismatches FROM stock_check_runs WHERE id = $1",
        id
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let mut mismatches: Vec<Mismatch> =
        serde_json::from_value(row.mismatches).map_err(anyhow::Error::from)?;
    let ids: Vec<Uuid> = mismatches.iter().map(|m| m.product_id).collect();
    let names = sqlx::query!(
        r#"SELECT p.id, p.name, pi.normalized_value AS "code?"
           FROM products p
           LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
           WHERE p.id = ANY($1)"#,
        &ids
    )
    .fetch_all(db)
    .await?;
    for m in &mut mismatches {
        if let Some(n) = names.iter().find(|n| n.id == m.product_id) {
            m.name = n.name.clone();
            m.code = n.code.clone();
        }
    }
    Ok(Some(CheckRun {
        id: row.id,
        ran_at: row.ran_at,
        mismatches,
    }))
}

pub async fn latest(db: &PgPool) -> AppResult<Option<CheckRun>> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM stock_check_runs ORDER BY ran_at DESC, id DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await?;
    match id {
        Some(id) => load(db, id).await,
        None => Ok(None),
    }
}

/// Runs forever: sleep until 02:00 IST, check, repeat. Failures are logged, never fatal.
pub async fn nightly(db: PgPool) {
    loop {
        let next = next_run_after(Utc::now());
        let wait = (next - Utc::now()).to_std().unwrap_or_default();
        tokio::time::sleep(wait).await;
        match run(&db).await {
            Ok(r) => tracing::info!(mismatches = r.mismatches.len(), "nightly stock check done"),
            Err(e) => tracing::error!(error = ?e, "nightly stock check failed"),
        }
    }
}

async fn latest_route(
    State(s): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Option<CheckRun>>> {
    user.require("inventory.read")?;
    Ok(Json(latest(&s.db).await?))
}

async fn run_route(
    State(s): State<AppState>,
    user: CurrentUser,
) -> AppResult<(StatusCode, Json<CheckRun>)> {
    user.require("inventory.adjust")?;
    Ok((StatusCode::CREATED, Json(run(&s.db).await?)))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stock-checks", axum::routing::post(run_route))
        .route("/stock-checks/latest", get(latest_route))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn next_run_after_cases() {
        // 15:30 IST → tonight 02:00 IST (20:30 UTC).
        assert_eq!(
            next_run_after(t("2026-11-06T10:00:00Z")),
            t("2026-11-06T20:30:00Z")
        );
        // Exactly 02:00 IST → strictly after, so the next night.
        assert_eq!(
            next_run_after(t("2026-11-06T20:30:00Z")),
            t("2026-11-07T20:30:00Z")
        );
        assert_eq!(
            next_run_after(t("2026-11-06T20:29:00Z")),
            t("2026-11-06T20:30:00Z")
        );
        // 01:00 IST → 02:00 IST the same IST day.
        assert_eq!(
            next_run_after(t("2026-11-06T19:30:00Z")),
            t("2026-11-06T20:30:00Z")
        );
    }
}
