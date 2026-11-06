//! Return items from a sale (spec §2, 5b Rulings 3–5): what the customer owes is reduced first,
//! the rest is refunded in cash or UPI, sellable items go back on the shelf. One transaction,
//! safe to retry; a preview runs the same checks and maths and rolls back.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

use super::{
    read,
    undo::{return_line_amount, split_return},
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    inventory::{
        domain::{MovementType, SourceType},
        ledger::{self as stock, MovementInput, Source},
        line_field,
    },
    ledger::{self, Account, Method, TxnInput, TxnKind},
    shared::{
        idempotency::{self, Begin},
        ids::new_id,
    },
};

#[derive(Deserialize)]
struct ReturnRequest {
    #[serde(default)]
    lines: Vec<LineRequest>,
    refund_method: Option<String>,
    note: Option<String>,
    #[serde(default)]
    preview: bool,
}

#[derive(Deserialize)]
struct LineRequest {
    order_line_id: Uuid,
    quantity: i64,
    disposition: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Disposition {
    Sellable,
    Damaged,
}

impl Disposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sellable => "sellable",
            Self::Damaged => "damaged",
        }
    }
}

struct ValidLine {
    order_line_id: Uuid,
    quantity: i64,
    disposition: Disposition,
}

struct Valid {
    lines: Vec<ValidLine>,
    refund_method: Option<Method>,
    note: Option<String>,
}

/// One line of the return, priced.
struct Priced {
    order_line_id: Uuid,
    product_id: Uuid,
    quantity: i64,
    disposition: Disposition,
    amount: Decimal,
}

/// The return's maths, worked out under the locks.
struct Plan {
    customer_id: Option<Uuid>,
    lines: Vec<Priced>,
    amount: Decimal,
    owed_reduced: Decimal,
    refund: Decimal,
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

fn field(name: impl Into<String>, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: name.into(),
        message: message.into(),
    }
}

fn parse(body: Value) -> AppResult<ReturnRequest> {
    serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))
}

/// Checks that need no database: lines present and unique, dispositions, method, note.
fn validate(req: &ReturnRequest) -> AppResult<Valid> {
    if req.lines.is_empty() {
        return Err(field("lines", "Add at least one item to return."));
    }
    let mut seen = HashSet::new();
    let mut lines = Vec::with_capacity(req.lines.len());
    for (i, l) in req.lines.iter().enumerate() {
        if !seen.insert(l.order_line_id) {
            return Err(line_field(
                i,
                "order_line_id",
                "This item is already on the return. Change that line instead.",
            ));
        }
        let disposition = match l.disposition.as_str() {
            "sellable" => Disposition::Sellable,
            "damaged" => Disposition::Damaged,
            _ => {
                return Err(line_field(i, "disposition", "Pick sellable or damaged."));
            }
        };
        lines.push(ValidLine {
            order_line_id: l.order_line_id,
            quantity: l.quantity,
            disposition,
        });
    }
    let refund_method = match req.refund_method.as_deref() {
        None => None,
        Some("cash") => Some(Method::Cash),
        Some("upi") => Some(Method::Upi),
        Some(_) => {
            return Err(field("refund_method", "Choose cash or UPI for the refund."));
        }
    };
    let note = req
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if note.as_ref().is_some_and(|n| n.chars().count() > 500) {
        return Err(field("note", "Keep the note to 500 characters or fewer."));
    }
    Ok(Valid {
        lines,
        refund_method,
        note,
    })
}

/// Locks the customer, then the order, then prices the lines against what is left to return
/// and splits the total between what is owed and a refund.
async fn plan(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    v: &Valid,
    preview: bool,
) -> AppResult<Plan> {
    let customer_id = sqlx::query_scalar!("SELECT customer_id FROM orders WHERE id = $1", order_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(cid) = customer_id {
        sqlx::query_scalar!("SELECT id FROM customers WHERE id = $1 FOR UPDATE", cid)
            .fetch_one(&mut **tx)
            .await?;
    }
    let status = sqlx::query_scalar!(
        "SELECT status FROM orders WHERE id = $1 FOR UPDATE",
        order_id
    )
    .fetch_one(&mut **tx)
    .await?;
    if status == "cancelled" {
        return Err(AppError::not_cancellable());
    }
    // A delivery order takes returns only once it reached the customer (Ruling 5). The lock
    // keeps the lock order customer → order → delivery.
    let delivery = sqlx::query_scalar!(
        "SELECT status FROM deliveries WHERE order_id = $1 FOR UPDATE",
        order_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    if delivery.is_some_and(|s| s != "delivered") {
        return Err(AppError::delivery_not_done());
    }

    let ids: Vec<Uuid> = v.lines.iter().map(|l| l.order_line_id).collect();
    let rows = sqlx::query!(
        r#"SELECT ol.id, ol.product_id, ol.quantity, ol.net_amount,
                  COALESCE((SELECT SUM(rl.quantity) FROM return_lines rl
                            WHERE rl.order_line_id = ol.id), 0)::BIGINT AS "returned!"
           FROM order_lines ol
           WHERE ol.order_id = $1 AND ol.id = ANY($2)"#,
        order_id,
        &ids
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut lines = Vec::with_capacity(v.lines.len());
    for (i, l) in v.lines.iter().enumerate() {
        let row = rows
            .iter()
            .find(|r| r.id == l.order_line_id)
            .ok_or_else(|| line_field(i, "order_line_id", "This item isn't on this sale."))?;
        let left = row.quantity - row.returned;
        if l.quantity < 1 {
            return Err(line_field(i, "quantity", "Enter 1 or more."));
        }
        if left < 1 {
            return Err(line_field(
                i,
                "quantity",
                "This item was already returned in full.",
            ));
        }
        if l.quantity > left {
            return Err(line_field(
                i,
                "quantity",
                format!("Only {left} left to return."),
            ));
        }
        lines.push(Priced {
            order_line_id: l.order_line_id,
            product_id: row.product_id,
            quantity: l.quantity,
            disposition: l.disposition,
            amount: return_line_amount(row.net_amount, row.quantity, row.returned, l.quantity),
        });
    }
    let amount: Decimal = lines.iter().map(|l| l.amount).sum();
    if amount.is_zero() {
        return Err(field(
            "lines",
            "These items were free, so there's nothing to return.",
        ));
    }
    let owed = ledger::order_owed(tx, order_id).await?;
    let (owed_reduced, refund) = split_return(amount, owed);
    if refund > Decimal::ZERO && v.refund_method.is_none() && !preview {
        return Err(field("refund_method", "Choose cash or UPI for the refund."));
    }
    Ok(Plan {
        customer_id,
        lines,
        amount,
        owed_reduced,
        refund,
    })
}

/// Works out a return without writing anything.
async fn preview(db: &PgPool, order_id: Uuid, req: &ReturnRequest) -> AppResult<Value> {
    let v = validate(req)?;
    let mut tx = db.begin().await?;
    let p = plan(&mut tx, order_id, &v, true).await?;
    tx.rollback().await?;
    Ok(json!({
        "amount": money(p.amount),
        "owed_reduced": money(p.owed_reduced),
        "refund": money(p.refund),
        "lines": p.lines.iter().map(|l| json!({
            "order_line_id": l.order_line_id,
            "quantity": l.quantity,
            "amount": money(l.amount),
        })).collect::<Vec<_>>(),
    }))
}

/// Takes back items of order `order_id`. Returns `(created, response)`: `created` is false
/// for a replay.
pub async fn create_return(
    db: &PgPool,
    actor: Uuid,
    order_id: Uuid,
    key: &str,
    body: Value,
) -> AppResult<(bool, Value)> {
    let hash = idempotency::hash_body(&body);
    let scope = format!("order_return:{order_id}");
    let mut tx = db.begin().await?;
    if let Begin::Replay(v) = idempotency::begin(&mut tx, actor, &scope, key, &hash).await? {
        return Ok((false, v));
    }
    let req = parse(body)?;
    let v = validate(&req)?;
    let p = plan(&mut tx, order_id, &v, false).await?;
    // Only kept when money is handed back, as the table requires.
    let refund_method = if p.refund > Decimal::ZERO {
        v.refund_method
    } else {
        None
    };
    let method_str = refund_method.map(|m| match m {
        Method::Upi => "upi",
        _ => "cash",
    });

    let return_id = new_id();
    sqlx::query!(
        "INSERT INTO returns
           (id, order_id, refund_method, amount, owed_reduced, refunded, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        return_id,
        order_id,
        method_str,
        p.amount,
        p.owed_reduced,
        p.refund,
        v.note,
        actor
    )
    .execute(&mut *tx)
    .await?;
    for l in &p.lines {
        sqlx::query!(
            "INSERT INTO return_lines (id, return_id, order_line_id, quantity, disposition, amount)
             VALUES ($1, $2, $3, $4, $5, $6)",
            new_id(),
            return_id,
            l.order_line_id,
            l.quantity,
            l.disposition.as_str(),
            l.amount
        )
        .execute(&mut *tx)
        .await?;
    }

    // Money: reduce what is owed first, then hand back the rest.
    if p.owed_reduced > Decimal::ZERO {
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::ReturnCredit,
                order_id: Some(order_id),
                customer_id: p.customer_id,
                method: Method::Internal,
                actor,
                idempotency_key: format!("return:{return_id}:credit"),
                note: None,
                reverses_id: None,
                entries: vec![
                    (Account::SalesReturns, p.owed_reduced),
                    (Account::Receivable(order_id), -p.owed_reduced),
                ],
            },
        )
        .await?;
    }
    if let Some(method) = refund_method {
        let account = if method == Method::Upi {
            Account::Upi
        } else {
            Account::Cash
        };
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::Refund,
                order_id: Some(order_id),
                customer_id: p.customer_id,
                method,
                actor,
                idempotency_key: format!("return:{return_id}:refund"),
                note: None,
                reverses_id: None,
                entries: vec![(Account::SalesReturns, p.refund), (account, -p.refund)],
            },
        )
        .await?;
    }

    // Stock: sellable items back on the shelf; unknown cost keeps the average.
    let moves: Vec<MovementInput> = p
        .lines
        .iter()
        .filter(|l| l.disposition == Disposition::Sellable)
        .map(|l| MovementInput {
            product_id: l.product_id,
            movement_type: MovementType::SaleReturn,
            quantity: l.quantity,
            unit_cost: None,
            reason: None,
        })
        .collect();
    if !moves.is_empty() {
        stock::apply(
            &mut tx,
            actor,
            Source {
                source_type: SourceType::Return,
                source_id: return_id,
            },
            &moves,
        )
        .await?;
    }

    let response = json!({
        "id": return_id,
        "order_id": order_id,
        "amount": money(p.amount),
        "owed_reduced": money(p.owed_reduced),
        "refund": money(p.refund),
        "refund_method": method_str,
        "note": v.note,
        "lines": p.lines.iter().map(|l| json!({
            "order_line_id": l.order_line_id,
            "quantity": l.quantity,
            "disposition": l.disposition.as_str(),
            "amount": money(l.amount),
        })).collect::<Vec<_>>(),
    });
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "order.return",
            entity_type: "order",
            entity_id: Some(order_id),
            before: None,
            after: Some(response.clone()),
        },
    )
    .await?;
    idempotency::finish(&mut tx, actor, &scope, key, &response).await?;
    tx.commit().await?;
    Ok((true, response))
}

pub async fn return_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<Response> {
    user.require("returns.create")?;
    // A preview writes nothing, so it needs no Idempotency-Key.
    let req = parse(body.clone())?;
    if req.preview {
        let v = preview(&s.db, id, &req).await?;
        return Ok((StatusCode::OK, Json(v)).into_response());
    }
    let key = idempotency::key_from_headers(&headers)?;
    let (created, _) = create_return(&s.db, user.id, id, &key, body).await?;
    // Rebuilt rather than replayed so cost visibility follows the caller.
    let mut conn = s.db.acquire().await?;
    let detail = read::order_detail(&mut conn, id, user.can("products.update"), &s.config).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(detail)).into_response())
}
