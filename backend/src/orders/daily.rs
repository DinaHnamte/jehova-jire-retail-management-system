//! Daily sales summary for the shop's IST day (spec §4).
use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Days, NaiveDate, NaiveTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    admin::session::CurrentUser,
    error::{AppError, AppResult},
    inventory::{ist, today_ist},
};

#[derive(Debug, Deserialize)]
pub struct DailyParams {
    pub date: Option<String>,
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

fn ist_start(d: NaiveDate) -> DateTime<Utc> {
    ist()
        .from_local_datetime(&d.and_time(NaiveTime::MIN))
        .single()
        .expect("fixed offset has no gaps")
        .with_timezone(&Utc)
}

pub async fn daily_route(
    State(s): State<AppState>,
    user: CurrentUser,
    Query(p): Query<DailyParams>,
) -> AppResult<Json<Value>> {
    user.require("reports.read")?;
    let date = match p.date.as_deref() {
        None => today_ist(),
        Some(d) => {
            NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| AppError::FieldInvalid {
                field: "date".into(),
                message: "Enter the date as YYYY-MM-DD.".into(),
            })?
        }
    };
    let from = ist_start(date);
    let until =
        ist_start(
            date.checked_add_days(Days::new(1))
                .ok_or_else(|| AppError::FieldInvalid {
                    field: "date".into(),
                    message: "Enter a date that exists.".into(),
                })?,
        );

    let sales = sqlx::query!(
        r#"SELECT count(*) AS "count!",
                  COALESCE(SUM(o.total), 0) AS "total!",
                  COALESCE(SUM(o.delivery_fee), 0) AS "delivery_fees!",
                  COALESCE(SUM((SELECT SUM(l.quantity * l.list_price) FROM order_lines l
                                 WHERE l.order_id = o.id)), 0) AS "list_total!"
           FROM orders o
           WHERE o.created_at >= $1 AND o.created_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    let received = sqlx::query!(
        r#"SELECT COALESCE(SUM(e.amount) FILTER (WHERE e.account = 'cash' AND e.amount > 0), 0) AS "cash!",
                  COALESCE(SUM(e.amount) FILTER (WHERE e.account = 'upi' AND e.amount > 0), 0) AS "upi!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           WHERE t.kind = 'payment' AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    let new_owed = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(e.amount), 0) AS "owed!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           JOIN orders o ON o.id = e.ref_id
           WHERE e.account = 'receivable' AND t.kind IN ('sale', 'payment')
             AND o.created_at >= $1 AND o.created_at < $2
             AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    // Each payment clears receivables; split what it cleared by whether the sale was made
    // that day, so sales total = paid on these sales + new owed, and cash + UPI = paid on
    // these sales + received on older sales.
    let applied = sqlx::query!(
        r#"SELECT COALESCE(-SUM(e.amount) FILTER (WHERE o.created_at >= $1 AND o.created_at < $2), 0) AS "these!",
                  COALESCE(-SUM(e.amount) FILTER (WHERE NOT (o.created_at >= $1 AND o.created_at < $2)), 0) AS "older!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           JOIN orders o ON o.id = e.ref_id
           WHERE t.kind = 'payment' AND e.account = 'receivable'
             AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    // Undo actions are reported on the day they happen; earlier days never change.
    let cancelled = sqlx::query!(
        r#"SELECT count(*) AS "count!",
                  COALESCE(SUM(o.total - o.delivery_fee + COALESCE(d.fee, 0)), 0) AS "total!"
           FROM orders o
           LEFT JOIN deliveries d ON d.order_id = o.id
           WHERE o.cancelled_at >= $1 AND o.cancelled_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    let returns_total = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(amount), 0) AS "total!"
           FROM returns WHERE created_at >= $1 AND created_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    let refunds = sqlx::query!(
        r#"SELECT COALESCE(-SUM(e.amount) FILTER (WHERE e.account = 'cash'), 0) AS "cash!",
                  COALESCE(-SUM(e.amount) FILTER (WHERE e.account = 'upi'), 0) AS "upi!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           WHERE t.kind = 'refund' AND e.account IN ('cash', 'upi') AND e.amount < 0
             AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    // Credits to receivable from cancels and return credits (negative amounts) minus refund
    // debits to receivable. Negated so a positive number means what is owed went down.
    let owed_reduced = sqlx::query_scalar!(
        r#"SELECT COALESCE(-SUM(e.amount), 0) AS "reduced!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           WHERE e.account = 'receivable' AND t.kind IN ('cancel', 'return_credit', 'refund')
             AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    // Fee edits after checkout: the receivable side of each fee_adjustment, signed.
    let fee_changes = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(e.amount), 0) AS "n!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           WHERE e.account = 'receivable' AND t.kind = 'fee_adjustment'
             AND t.occurred_at >= $1 AND t.occurred_at < $2"#,
        from,
        until
    )
    .fetch_one(&s.db)
    .await?;
    let total_owed = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(amount), 0) AS "owed!" FROM ledger_entries WHERE account = 'receivable'"#
    )
    .fetch_one(&s.db)
    .await?;

    Ok(Json(json!({
        "date": date.to_string(),
        "sales_count": sales.count,
        "sales_total": money(sales.total),
        "delivery_fees": money(sales.delivery_fees),
        "fee_changes": money(fee_changes),
        "discounts": money(sales.list_total - (sales.total - sales.delivery_fees)),
        "cash_received": money(received.cash),
        "upi_received": money(received.upi),
        "paid_on_these_sales": money(applied.these),
        "received_on_older_sales": money(applied.older),
        "new_owed": money(new_owed),
        "total_owed": money(total_owed),
        "cancelled_count": cancelled.count,
        "cancelled_total": money(cancelled.total),
        "returns_total": money(returns_total),
        "refunds_cash": money(refunds.cash),
        "refunds_upi": money(refunds.upi),
        "owed_reduced": money(owed_reduced),
    })))
}
