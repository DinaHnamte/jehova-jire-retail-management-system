//! Cancel a delivery before pickup, or check the goods of a failed one back in (spec §3.3). Both
//! go through the sale cancel (`orders::cancel`) so the money, stock and order are reversed in
//! the same transaction as the delivery change.
use axum::{Json, extract::State, http::HeaderMap};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::{
    actions::{Event, event},
    domain::{Action, next_status},
    input::optional_money,
    read::{self, DeliveryDetail, money},
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
    orders::cancel::{cancel, cancel_core, lock_order},
    shared::{
        idempotency::{self, Begin},
        ids::new_id,
    },
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckInRequest {
    #[serde(default)]
    lines: Vec<CheckInLine>,
    /// Pay for the failed trip; ₹0 when left out.
    trip_pay: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckInLine {
    order_line_id: Uuid,
    disposition: String,
}

const EVERY_LINE: &str = "Choose sellable or damaged for every item.";

fn every_line() -> AppError {
    AppError::FieldInvalid {
        field: "lines".into(),
        message: EVERY_LINE.into(),
    }
}

/// Checks the body without the database: each disposition, and no line twice. Returns
/// `order_line_id → damaged`.
fn validate(req: &CheckInRequest) -> AppResult<HashMap<Uuid, bool>> {
    let mut out = HashMap::with_capacity(req.lines.len());
    for (i, l) in req.lines.iter().enumerate() {
        let damaged = match l.disposition.as_str() {
            "sellable" => false,
            "damaged" => true,
            _ => return Err(line_field(i, "disposition", "Pick sellable or damaged.")),
        };
        if out.insert(l.order_line_id, damaged).is_some() {
            return Err(every_line());
        }
    }
    if out.is_empty() {
        return Err(every_line());
    }
    Ok(out)
}

/// failed → returned: the sale is cancelled ("Delivery returned"), damaged lines go out again
/// as `damage`, the failed trip's pay is set. Returns once done or when `key` was already used
/// for the same body.
pub async fn check_in(db: &PgPool, actor: Uuid, id: Uuid, key: &str, body: Value) -> AppResult<()> {
    let hash = idempotency::hash_body(&body);
    let scope = format!("delivery_check_in:{id}");
    let mut tx = db.begin().await?;
    if let Begin::Replay(_) = idempotency::begin(&mut tx, actor, &scope, key, &hash).await? {
        return Ok(());
    }
    let req: CheckInRequest =
        serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let dispositions = validate(&req)?;
    let trip_pay = optional_money(&req.trip_pay, "trip_pay")?.unwrap_or(Decimal::ZERO);

    let order_id = sqlx::query_scalar!("SELECT order_id FROM deliveries WHERE id = $1", id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let order = lock_order(&mut tx, order_id).await?;
    let from = order
        .delivery
        .as_ref()
        .map(|d| d.status)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("delivery {id} not locked")))?;
    let to = next_status(from, Action::CheckIn).map_err(AppError::delivery_status)?;

    let lines = sqlx::query!(
        "SELECT id, product_id, quantity FROM order_lines WHERE order_id = $1 ORDER BY line_no",
        order_id
    )
    .fetch_all(&mut *tx)
    .await?;
    let on_order: HashSet<Uuid> = lines.iter().map(|l| l.id).collect();
    if on_order != dispositions.keys().copied().collect() {
        return Err(every_line());
    }

    let refunds = cancel_core(&mut tx, actor, &order, Some("Delivery returned")).await?;
    let damaged: Vec<MovementInput> = lines
        .iter()
        .filter(|l| dispositions[&l.id])
        .map(|l| MovementInput {
            product_id: l.product_id,
            movement_type: MovementType::Damage,
            quantity: l.quantity,
            unit_cost: None,
            reason: Some("Damaged on a returned delivery".into()),
        })
        .collect();
    if !damaged.is_empty() {
        stock::apply(
            &mut tx,
            actor,
            Source {
                source_type: SourceType::Delivery,
                source_id: id,
            },
            &damaged,
        )
        .await?;
    }

    sqlx::query!(
        "UPDATE delivery_attempts SET trip_pay = $2
         WHERE id = (SELECT id FROM delivery_attempts WHERE delivery_id = $1
                     ORDER BY occurred_at DESC, id DESC LIMIT 1)
           AND trip_pay IS NULL",
        id,
        trip_pay
    )
    .execute(&mut *tx)
    .await?;
    // The assignment already ended `failed`; nobody has the job.
    sqlx::query!(
        "UPDATE deliveries SET status = 'returned', closed_at = now(), updated_at = now()
         WHERE id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    event(
        &mut tx,
        Event {
            id: new_id(),
            delivery_id: id,
            from,
            to,
            kind: "status",
            actor,
            note: None,
            changes: None,
        },
    )
    .await?;
    let line_out: Vec<Value> = lines
        .iter()
        .map(|l| {
            json!({
                "order_line_id": l.id,
                "disposition": if dispositions[&l.id] { "damaged" } else { "sellable" },
            })
        })
        .collect();
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "delivery.check_in",
            entity_type: "delivery",
            entity_id: Some(id),
            before: Some(json!({ "status": from.as_str() })),
            after: Some(json!({
                "status": to.as_str(), "lines": line_out, "trip_pay": money(trip_pay),
                "refunds": refunds,
            })),
        },
    )
    .await?;
    idempotency::finish(&mut tx, actor, &scope, key, &json!({ "id": id })).await?;
    tx.commit().await?;
    Ok(())
}

async fn detail(s: &AppState, id: Uuid) -> AppResult<Json<DeliveryDetail>> {
    let mut conn = s.db.acquire().await?;
    Ok(Json(read::delivery_detail(&mut conn, id, &s.config).await?))
}

/// Cancel before pickup (Ruling 4): the sale cancel for this delivery's order.
pub async fn cancel_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.cancel")?;
    let key = idempotency::key_from_headers(&headers)?;
    let order_id = sqlx::query_scalar!("SELECT order_id FROM deliveries WHERE id = $1", id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(AppError::NotFound)?;
    cancel(&s.db, user.id, order_id, &key, body).await?;
    detail(&s, id).await
}

pub async fn check_in_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.check_in")?;
    let key = idempotency::key_from_headers(&headers)?;
    check_in(&s.db, user.id, id, &key, body).await?;
    detail(&s, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(lines: Value) -> CheckInRequest {
        serde_json::from_value(json!({ "lines": lines })).unwrap()
    }

    #[test]
    fn validate_rejects_duplicates_empty_and_bad_dispositions() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let ok = validate(&req(json!([
            {"order_line_id": a, "disposition": "sellable"},
            {"order_line_id": b, "disposition": "damaged"}
        ])))
        .unwrap();
        assert_eq!((ok[&a], ok[&b]), (false, true));
        let dup = validate(&req(json!([
            {"order_line_id": a, "disposition": "sellable"},
            {"order_line_id": a, "disposition": "damaged"}
        ])));
        assert!(matches!(dup, Err(AppError::FieldInvalid { field, .. }) if field == "lines"));
        assert!(validate(&req(json!([]))).is_err());
        let bad = validate(&req(json!([{"order_line_id": a, "disposition": "lost"}])));
        assert!(
            matches!(bad, Err(AppError::FieldInvalid { field, .. }) if field == "lines[0].disposition")
        );
    }
}
