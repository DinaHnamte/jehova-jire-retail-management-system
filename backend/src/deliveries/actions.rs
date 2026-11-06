//! The owner's dispatch actions: assign, edit before pickup, status steps, retry (spec §1, §3.2).
//! Each locks the delivery row and checks the status before writing. A fee edit moves money, so
//! it locks the customer, then the order, then the delivery, as cancel does.
use axum::{Json, extract::State, http::HeaderMap};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{
    domain::{Action, Collector, PayMethod, Status, next_status},
    input::{
        EditRequest, StatusChange, StatusRequest, check_collection, optional_money, validate_edit,
        validate_status,
    },
    read::{self, DeliveryDetail, money},
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    drivers::{DriverRef, active_driver},
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    ledger::{self, Account, Method, TxnInput, TxnKind},
    shared::{
        idempotency::{self, Begin},
        ids::new_id,
    },
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignRequest {
    pub driver_id: Uuid,
    pub driver_pay: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryRequest {
    pub driver_id: Uuid,
    pub driver_pay: Option<String>,
    /// Pay for the failed trip; ₹0 when left out.
    pub trip_pay: Option<String>,
}

/// The delivery row, read under `FOR UPDATE`.
struct Row {
    id: Uuid,
    status: String,
    recipient_name: String,
    recipient_phone: String,
    address_line: String,
    locality: String,
    landmark: Option<String>,
    instructions: Option<String>,
    fee: Decimal,
    driver_pay: Option<Decimal>,
    collector: String,
    pay_method: String,
    current_driver_id: Option<Uuid>,
}

impl Row {
    fn status(&self) -> AppResult<Status> {
        Status::parse(&self.status)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("delivery status {}", self.status)))
    }
}

async fn lock(tx: &mut PgConnection, id: Uuid) -> AppResult<Row> {
    sqlx::query_as!(
        Row,
        "SELECT id, status, recipient_name, recipient_phone, address_line, locality, landmark,
                instructions, fee, driver_pay, collector, pay_method, current_driver_id
         FROM deliveries WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(tx)
    .await?
    .ok_or(AppError::NotFound)
}

fn field(name: &str, message: &str) -> AppError {
    AppError::FieldInvalid {
        field: name.into(),
        message: message.into(),
    }
}

struct Event<'a> {
    id: Uuid,
    delivery_id: Uuid,
    from: Status,
    to: Status,
    /// `status` or `edit`.
    kind: &'a str,
    actor: Uuid,
    note: Option<&'a str>,
    changes: Option<Value>,
}

async fn event(tx: &mut PgConnection, e: Event<'_>) -> AppResult<()> {
    sqlx::query!(
        "INSERT INTO delivery_events (id, delivery_id, from_status, to_status, kind, actor_id,
                                      note, changes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        e.id,
        e.delivery_id,
        e.from.as_str(),
        e.to.as_str(),
        e.kind,
        e.actor,
        e.note,
        e.changes
    )
    .execute(tx)
    .await?;
    Ok(())
}

async fn end_assignment(tx: &mut PgConnection, delivery_id: Uuid, reason: &str) -> AppResult<()> {
    sqlx::query!(
        "UPDATE delivery_assignments SET ended_at = now(), end_reason = $2
         WHERE delivery_id = $1 AND ended_at IS NULL",
        delivery_id,
        reason
    )
    .execute(tx)
    .await?;
    Ok(())
}

async fn display_name(tx: &mut PgConnection, id: Uuid) -> AppResult<String> {
    Ok(
        sqlx::query_scalar!("SELECT display_name FROM users WHERE id = $1", id)
            .fetch_one(tx)
            .await?,
    )
}

/// Hands the job to `driver` and moves it to `assigned` (Ruling 7 for the pay). The row is
/// locked and the transition already checked.
async fn assign_core(
    tx: &mut PgConnection,
    actor: Uuid,
    row: &Row,
    from: Status,
    driver: &DriverRef,
    pay: Option<Decimal>,
) -> AppResult<Decimal> {
    let pay = pay.or(row.driver_pay).unwrap_or(driver.default_pay);
    end_assignment(tx, row.id, "reassigned").await?;
    sqlx::query!(
        "INSERT INTO delivery_assignments (id, delivery_id, driver_id, assigned_by)
         VALUES ($1, $2, $3, $4)",
        new_id(),
        row.id,
        driver.id,
        actor
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE deliveries
         SET status = 'assigned', current_driver_id = $2, assigned_at = now(), driver_pay = $3,
             updated_at = now()
         WHERE id = $1",
        row.id,
        driver.id,
        pay
    )
    .execute(&mut *tx)
    .await?;
    let note = format!("Assigned to {}", driver.display_name);
    event(
        tx,
        Event {
            id: new_id(),
            delivery_id: row.id,
            from,
            to: Status::Assigned,
            kind: "status",
            actor,
            note: Some(&note),
            changes: None,
        },
    )
    .await?;
    Ok(pay)
}

/// `to_send` → assigned, or a reassign to a different driver.
pub async fn assign(db: &PgPool, actor: Uuid, id: Uuid, req: AssignRequest) -> AppResult<()> {
    let pay = optional_money(&req.driver_pay, "driver_pay")?;
    let mut tx = db.begin().await?;
    let row = lock(&mut tx, id).await?;
    let from = row.status()?;
    let action = if from == Status::Assigned {
        Action::Reassign
    } else {
        Action::Assign
    };
    next_status(from, action).map_err(AppError::delivery_status)?;
    if row.current_driver_id == Some(req.driver_id) {
        return Err(field("driver_id", "This driver already has it."));
    }
    let driver = active_driver(&mut tx, req.driver_id).await?;
    let pay = assign_core(&mut tx, actor, &row, from, &driver, pay).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "delivery.assign",
            entity_type: "delivery",
            entity_id: Some(id),
            before: Some(json!({ "status": row.status, "driver_id": row.current_driver_id })),
            after: Some(json!({
                "status": "assigned", "driver_id": driver.id, "driver_pay": money(pay)
            })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// failed → assigned: the failed trip's pay is set, then the job goes out again.
pub async fn retry(db: &PgPool, actor: Uuid, id: Uuid, req: RetryRequest) -> AppResult<()> {
    let pay = optional_money(&req.driver_pay, "driver_pay")?;
    let trip_pay = optional_money(&req.trip_pay, "trip_pay")?.unwrap_or(Decimal::ZERO);
    let mut tx = db.begin().await?;
    let row = lock(&mut tx, id).await?;
    let from = row.status()?;
    next_status(from, Action::Retry).map_err(AppError::delivery_status)?;
    let driver = active_driver(&mut tx, req.driver_id).await?;
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
    let pay = assign_core(&mut tx, actor, &row, from, &driver, pay).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "delivery.retry",
            entity_type: "delivery",
            entity_id: Some(id),
            before: Some(json!({ "status": row.status })),
            after: Some(json!({
                "status": "assigned", "driver_id": driver.id, "driver_pay": money(pay),
                "trip_pay": money(trip_pay)
            })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Picked up, delivered or couldn't deliver. With `by_driver`, the job must be that driver's
/// current one or it is 404 (no hint that it exists). Without, the owner acts for the driver
/// and the event says so. Runs inside the caller's transaction.
pub async fn change_status(
    tx: &mut PgConnection,
    actor: Uuid,
    delivery_id: Uuid,
    change: StatusChange,
    by_driver: Option<Uuid>,
) -> AppResult<()> {
    let row = lock(tx, delivery_id).await?;
    if let Some(me) = by_driver
        && row.current_driver_id != Some(me)
    {
        // A repeated tap after this driver finished the job is a conflict the app treats as
        // done. Anything else (reassigned, cancelled, never theirs) gives no hint it exists.
        let last = sqlx::query!(
            "SELECT driver_id, end_reason FROM delivery_assignments
             WHERE delivery_id = $1 ORDER BY assigned_at DESC, id DESC LIMIT 1",
            delivery_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let finished_by_me = last.is_some_and(|a| {
            a.driver_id == me && matches!(a.end_reason.as_deref(), Some("completed" | "failed"))
        });
        return Err(if finished_by_me {
            AppError::delivery_status(row.status()?)
        } else {
            AppError::NotFound
        });
    }
    let from = row.status()?;
    let action = match &change {
        StatusChange::PickedUp => Action::PickUp,
        StatusChange::Delivered { .. } => Action::Deliver,
        StatusChange::Failed { .. } => Action::Fail,
    };
    let to = next_status(from, action).map_err(AppError::delivery_status)?;
    // Every step from here starts at assigned or picked up, which always have a driver.
    let driver_id = row.current_driver_id.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("delivery {delivery_id} has no driver"))
    })?;
    let after = match &change {
        StatusChange::PickedUp => {
            sqlx::query!(
                "UPDATE deliveries SET status = 'picked_up', picked_up_at = now(),
                                       updated_at = now()
                 WHERE id = $1",
                delivery_id
            )
            .execute(&mut *tx)
            .await?;
            json!({ "status": to.as_str() })
        }
        StatusChange::Delivered { received_by } => {
            sqlx::query!(
                "UPDATE deliveries
                 SET status = 'delivered', delivered_at = now(), received_by = $2,
                     current_driver_id = NULL, updated_at = now()
                 WHERE id = $1",
                delivery_id,
                received_by
            )
            .execute(&mut *tx)
            .await?;
            end_assignment(tx, delivery_id, "completed").await?;
            json!({ "status": to.as_str(), "received_by": received_by })
        }
        StatusChange::Failed { reason, note } => {
            sqlx::query!(
                "INSERT INTO delivery_attempts (id, delivery_id, driver_id, reason, note,
                                                recorded_by)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                new_id(),
                delivery_id,
                driver_id,
                reason.as_str(),
                note.as_deref(),
                actor
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE deliveries SET status = 'failed', current_driver_id = NULL,
                                       updated_at = now()
                 WHERE id = $1",
                delivery_id
            )
            .execute(&mut *tx)
            .await?;
            end_assignment(tx, delivery_id, "failed").await?;
            json!({ "status": to.as_str(), "reason": reason.as_str(), "note": note })
        }
    };
    let note = match by_driver {
        Some(_) => None,
        None => Some(format!(
            "Marked by {} for {}",
            display_name(tx, actor).await?,
            display_name(tx, driver_id).await?
        )),
    };
    event(
        tx,
        Event {
            id: new_id(),
            delivery_id,
            from,
            to,
            kind: "status",
            actor,
            note: note.as_deref(),
            changes: None,
        },
    )
    .await?;
    audit::record(
        tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "delivery.status",
            entity_type: "delivery",
            entity_id: Some(delivery_id),
            before: Some(json!({ "status": from.as_str(), "driver_id": driver_id })),
            after: Some(after),
        },
    )
    .await?;
    Ok(())
}

/// Records `[old, new]` under `name` when they differ.
fn diff<T: PartialEq + Into<Value> + Clone>(
    changes: &mut Map<String, Value>,
    name: &str,
    old: T,
    new: &T,
) {
    if old != *new {
        changes.insert(name.into(), json!([old.into(), new.clone().into()]));
    }
}

fn opt_money(d: Option<Decimal>) -> Value {
    d.map_or(Value::Null, |d| Value::String(money(d)))
}

/// Edit before pickup (spec §3.2). A fee change posts the difference (Ruling 6). Returns once
/// done or when `key` was already used for the same body.
pub async fn edit(db: &PgPool, actor: Uuid, id: Uuid, key: &str, body: Value) -> AppResult<()> {
    let hash = idempotency::hash_body(&body);
    let scope = format!("delivery_edit:{id}");
    let mut tx = db.begin().await?;
    if let Begin::Replay(_) = idempotency::begin(&mut tx, actor, &scope, key, &hash).await? {
        return Ok(());
    }
    let req: EditRequest =
        serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let e = validate_edit(&req)?;

    // Money may move: customer, then order, then delivery.
    let mut customer_id = None;
    let order_id = sqlx::query_scalar!("SELECT order_id FROM deliveries WHERE id = $1", id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    if e.fee.is_some() {
        customer_id = sqlx::query_scalar!("SELECT customer_id FROM orders WHERE id = $1", order_id)
            .fetch_one(&mut *tx)
            .await?;
        if let Some(cid) = customer_id {
            sqlx::query_scalar!("SELECT id FROM customers WHERE id = $1 FOR UPDATE", cid)
                .fetch_one(&mut *tx)
                .await?;
        }
        sqlx::query_scalar!("SELECT id FROM orders WHERE id = $1 FOR UPDATE", order_id)
            .fetch_one(&mut *tx)
            .await?;
    }
    let row = lock(&mut tx, id).await?;
    let status = row.status()?;
    if !matches!(status, Status::ToSend | Status::Assigned) {
        return Err(AppError::delivery_status(status));
    }

    let old_collector = Collector::parse(&row.collector)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("collector {}", row.collector)))?;
    let old_pay_method = PayMethod::parse(&row.pay_method)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("pay method {}", row.pay_method)))?;
    let collector = e.collector.unwrap_or(old_collector);
    // As at checkout, the shop collecting means the shop pays the driver unless told otherwise.
    let pay_method = e.pay_method.unwrap_or(
        if collector == Collector::Shop && old_pay_method == PayMethod::RetainFromCollection {
            PayMethod::PaidByShop
        } else {
            old_pay_method
        },
    );
    check_collection("", collector, pay_method)?;
    let driver_pay = e.driver_pay.unwrap_or(row.driver_pay);
    if driver_pay.is_none() && status == Status::Assigned {
        return Err(field("driver_pay", "Enter the driver's pay, or 0."));
    }
    let new = Row {
        id,
        status: row.status.clone(),
        recipient_name: e
            .recipient_name
            .unwrap_or_else(|| row.recipient_name.clone()),
        recipient_phone: e
            .recipient_phone
            .unwrap_or_else(|| row.recipient_phone.clone()),
        address_line: e.address_line.unwrap_or_else(|| row.address_line.clone()),
        locality: e.locality.unwrap_or_else(|| row.locality.clone()),
        landmark: e.landmark.unwrap_or_else(|| row.landmark.clone()),
        instructions: e.instructions.unwrap_or_else(|| row.instructions.clone()),
        fee: e.fee.unwrap_or(row.fee),
        driver_pay,
        collector: collector.as_str().into(),
        pay_method: pay_method.as_str().into(),
        current_driver_id: row.current_driver_id,
    };

    let mut changes = Map::new();
    diff(
        &mut changes,
        "recipient_name",
        row.recipient_name.clone(),
        &new.recipient_name,
    );
    diff(
        &mut changes,
        "recipient_phone",
        row.recipient_phone.clone(),
        &new.recipient_phone,
    );
    diff(
        &mut changes,
        "address_line",
        row.address_line.clone(),
        &new.address_line,
    );
    diff(
        &mut changes,
        "locality",
        row.locality.clone(),
        &new.locality,
    );
    diff(
        &mut changes,
        "landmark",
        row.landmark.clone(),
        &new.landmark,
    );
    diff(
        &mut changes,
        "instructions",
        row.instructions.clone(),
        &new.instructions,
    );
    diff(&mut changes, "fee", money(row.fee), &money(new.fee));
    diff(
        &mut changes,
        "driver_pay",
        opt_money(row.driver_pay),
        &opt_money(new.driver_pay),
    );
    diff(
        &mut changes,
        "collector",
        row.collector.clone(),
        &new.collector,
    );
    diff(
        &mut changes,
        "pay_method",
        row.pay_method.clone(),
        &new.pay_method,
    );

    let fee_change = new.fee - row.fee;
    if !fee_change.is_zero() {
        let owed = ledger::order_owed(&mut tx, order_id).await?;
        if owed + fee_change < Decimal::ZERO {
            return Err(field(
                "fee",
                "The customer has already paid more than that. Cancel the delivery or return items to give money back.",
            ));
        }
    }
    if changes.is_empty() {
        idempotency::finish(&mut tx, actor, &scope, key, &json!({ "id": id })).await?;
        tx.commit().await?;
        return Ok(());
    }

    sqlx::query!(
        "UPDATE deliveries
         SET recipient_name = $2, recipient_phone = $3, address_line = $4, locality = $5,
             landmark = $6, instructions = $7, fee = $8, driver_pay = $9, collector = $10,
             pay_method = $11, updated_at = now()
         WHERE id = $1",
        id,
        new.recipient_name,
        new.recipient_phone,
        new.address_line,
        new.locality,
        new.landmark,
        new.instructions,
        new.fee,
        new.driver_pay,
        new.collector,
        new.pay_method
    )
    .execute(&mut *tx)
    .await?;
    let event_id = new_id();
    let changes = Value::Object(changes);
    event(
        &mut tx,
        Event {
            id: event_id,
            delivery_id: id,
            from: status,
            to: status,
            kind: "edit",
            actor,
            note: None,
            changes: Some(changes.clone()),
        },
    )
    .await?;
    if !fee_change.is_zero() {
        // Fee up: the customer owes more. Fee down: the reverse.
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::FeeAdjustment,
                order_id: Some(order_id),
                customer_id,
                method: Method::Internal,
                actor,
                idempotency_key: format!("delivery:{id}:fee:{event_id}"),
                note: None,
                reverses_id: None,
                entries: vec![
                    (Account::Receivable(order_id), fee_change),
                    (Account::DeliveryFeeIncome, -fee_change),
                ],
            },
        )
        .await?;
    }
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "delivery.edit",
            entity_type: "delivery",
            entity_id: Some(id),
            before: None,
            after: Some(json!({ "changes": changes })),
        },
    )
    .await?;
    idempotency::finish(
        &mut tx,
        actor,
        &scope,
        key,
        &json!({ "id": id, "event_id": event_id }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The detail after an action, rebuilt rather than replayed.
async fn detail(s: &AppState, id: Uuid) -> AppResult<Json<DeliveryDetail>> {
    let mut conn = s.db.acquire().await?;
    Ok(Json(read::delivery_detail(&mut conn, id, &s.config).await?))
}

pub async fn assign_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<AssignRequest>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.assign")?;
    assign(&s.db, user.id, id, req).await?;
    detail(&s, id).await
}

pub async fn retry_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<RetryRequest>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.assign")?;
    retry(&s.db, user.id, id, req).await?;
    detail(&s, id).await
}

pub async fn edit_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.assign")?;
    let key = idempotency::key_from_headers(&headers)?;
    edit(&s.db, user.id, id, &key, body).await?;
    detail(&s, id).await
}

/// Status on a driver's behalf.
pub async fn status_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<StatusRequest>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.assign")?;
    let change = validate_status(&req)?;
    let mut tx = s.db.begin().await?;
    change_status(&mut tx, user.id, id, change, None).await?;
    tx.commit().await?;
    detail(&s, id).await
}
