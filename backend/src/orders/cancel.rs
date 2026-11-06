//! Cancel a sale (spec §2, 5b Ruling 1): stock goes back on the shelf, the sale is reversed and
//! whatever was paid on it is refunded by the method it came in, all in one transaction and safe
//! to retry.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::read;
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    inventory::{
        domain::{MovementType, SourceType},
        ledger::{self as stock, MovementInput, Source},
    },
    ledger::{self, Account, Method, TxnInput, TxnKind},
    shared::idempotency::{self, Begin},
};

#[derive(Deserialize)]
struct CancelRequest {
    reason: Option<String>,
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

/// Trimmed; empty is no reason; at most 500 characters.
fn clean_reason(reason: Option<&str>) -> AppResult<Option<String>> {
    let reason = reason.map(str::trim).filter(|s| !s.is_empty());
    if reason.is_some_and(|r| r.chars().count() > 500) {
        return Err(AppError::FieldInvalid {
            field: "reason".into(),
            message: "Keep the reason to 500 characters or fewer.".into(),
        });
    }
    Ok(reason.map(str::to_string))
}

/// Cancels order `order_id`. Returns `(cancelled, response)`: `cancelled` is false for a replay.
pub async fn cancel(
    db: &PgPool,
    actor: Uuid,
    order_id: Uuid,
    key: &str,
    body: Value,
) -> AppResult<(bool, Value)> {
    let hash = idempotency::hash_body(&body);
    let scope = format!("order_cancel:{order_id}");
    let mut tx = db.begin().await?;
    if let Begin::Replay(v) = idempotency::begin(&mut tx, actor, &scope, key, &hash).await? {
        return Ok((false, v));
    }
    let req: CancelRequest =
        serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let reason = clean_reason(req.reason.as_deref())?;

    // Lock the customer first, as later payments do, so a payment racing this cancel either
    // finishes before we read what was paid or waits and then finds nothing owed here.
    let customer_id = sqlx::query_scalar!("SELECT customer_id FROM orders WHERE id = $1", order_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(cid) = customer_id {
        sqlx::query_scalar!("SELECT id FROM customers WHERE id = $1 FOR UPDATE", cid)
            .fetch_one(&mut *tx)
            .await?;
    }
    let order = sqlx::query!(
        "SELECT status, total FROM orders WHERE id = $1 FOR UPDATE",
        order_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if order.status == "cancelled" {
        return Err(AppError::not_cancellable());
    }
    let has_returns = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM returns WHERE order_id = $1) AS "has!""#,
        order_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if has_returns {
        return Err(AppError::has_returns());
    }

    // What was paid on this order, per method. Later payments carry no order id but still
    // credit this order's receivable, so this reads entries, not transactions.
    let paid = sqlx::query!(
        r#"SELECT t.method AS "method!", -SUM(e.amount) AS "paid!"
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           WHERE e.account = 'receivable' AND e.ref_id = $1 AND t.kind = 'payment'
           GROUP BY t.method
           ORDER BY t.method"#,
        order_id
    )
    .fetch_all(&mut *tx)
    .await?;

    // Money: reverse the sale, then hand back what was paid, one refund per method.
    let total = order.total;
    if !total.is_zero() {
        let sale_id = sqlx::query_scalar!(
            "SELECT id FROM ledger_transactions WHERE order_id = $1 AND kind = 'sale'",
            order_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("order {order_id} has no sale")))?;
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::Cancel,
                order_id: Some(order_id),
                customer_id,
                method: Method::Internal,
                actor,
                idempotency_key: format!("order:{order_id}:cancel"),
                note: None,
                reverses_id: Some(sale_id),
                entries: vec![
                    (Account::SalesRevenue, total),
                    (Account::Receivable(order_id), -total),
                ],
            },
        )
        .await?;
    }
    let mut refunds = Vec::new();
    for p in paid.iter().filter(|p| p.paid > Decimal::ZERO) {
        let (method, account) = match p.method.as_str() {
            "cash" => (Method::Cash, Account::Cash),
            "upi" => (Method::Upi, Account::Upi),
            other => {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "payment with method {other}"
                )));
            }
        };
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::Refund,
                order_id: Some(order_id),
                customer_id,
                method,
                actor,
                idempotency_key: format!("order:{order_id}:cancel_refund:{}", p.method),
                note: None,
                reverses_id: None,
                entries: vec![(Account::Receivable(order_id), p.paid), (account, -p.paid)],
            },
        )
        .await?;
        refunds.push(json!({ "method": p.method, "amount": money(p.paid) }));
    }
    let owed = ledger::order_owed(&mut tx, order_id).await?;
    if !owed.is_zero() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "cancelled order {order_id} still owes {owed}"
        )));
    }

    // Stock: every line back in full; unknown cost keeps the average.
    let lines = sqlx::query!(
        "SELECT product_id, quantity FROM order_lines WHERE order_id = $1 ORDER BY line_no",
        order_id
    )
    .fetch_all(&mut *tx)
    .await?;
    let moves: Vec<MovementInput> = lines
        .iter()
        .map(|l| MovementInput {
            product_id: l.product_id,
            movement_type: MovementType::SaleCancel,
            quantity: l.quantity,
            unit_cost: None,
            reason: None,
        })
        .collect();
    stock::apply(
        &mut tx,
        actor,
        Source {
            source_type: SourceType::Order,
            source_id: order_id,
        },
        &moves,
    )
    .await?;

    sqlx::query!(
        "UPDATE orders
         SET status = 'cancelled', cancelled_at = now(), cancelled_by = $2, cancel_reason = $3
         WHERE id = $1",
        order_id,
        actor,
        reason
    )
    .execute(&mut *tx)
    .await?;

    let response = json!({
        "id": order_id,
        "status": "cancelled",
        "total": money(total),
        "refunds": refunds,
        "reason": reason,
    });
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "order.cancel",
            entity_type: "order",
            entity_id: Some(order_id),
            before: Some(json!({ "status": order.status })),
            after: Some(response.clone()),
        },
    )
    .await?;
    idempotency::finish(&mut tx, actor, &scope, key, &response).await?;
    tx.commit().await?;
    Ok((true, response))
}

pub async fn cancel_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<(StatusCode, Json<read::OrderDetail>)> {
    user.require("orders.void")?;
    let key = idempotency::key_from_headers(&headers)?;
    cancel(&s.db, user.id, id, &key, body).await?;
    // Rebuilt rather than replayed so cost visibility follows the caller.
    let mut conn = s.db.acquire().await?;
    let detail = read::order_detail(&mut conn, id, user.can("products.update"), &s.config).await?;
    Ok((StatusCode::OK, Json(detail)))
}
