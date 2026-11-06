//! Later payments: a customer pays down what they owe, applied to their oldest unpaid sales
//! first, as one ledger transaction (spec §3). Safe to retry.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    ledger::{self, Account, Method, TxnInput, TxnKind},
    orders::domain::allocate,
    shared::{
        idempotency::{self, Begin},
        ids::new_id,
        money::parse_money,
    },
};

#[derive(Deserialize)]
struct PaymentRequest {
    method: String,
    amount: String,
    note: Option<String>,
}

fn field(name: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: name.into(),
        message: message.into(),
    }
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

/// Returns `(created, response)`; `created` is false for a replay.
pub async fn record(
    db: &PgPool,
    actor: Uuid,
    customer_id: Uuid,
    key: &str,
    body: Value,
) -> AppResult<(bool, Value)> {
    // The customer is part of the request, so it is part of what the key is checked against.
    let hash = idempotency::hash_body(&json!({ "customer_id": customer_id, "body": body }));
    let scope = format!("customer_payment:{customer_id}");
    let mut tx = db.begin().await?;
    if let Begin::Replay(v) = idempotency::begin(&mut tx, actor, &scope, key, &hash).await? {
        return Ok((false, v));
    }
    let req: PaymentRequest =
        serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let method = match req.method.as_str() {
        "cash" => Method::Cash,
        "upi" => Method::Upi,
        _ => return Err(field("method", "Pick cash or UPI.")),
    };
    let amount = parse_money(&req.amount).map_err(|m| field("amount", m))?;
    if amount.is_zero() {
        return Err(field("amount", "Enter an amount more than 0."));
    }
    let note = req.note.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if note.is_some_and(|n| n.chars().count() > 500) {
        return Err(field("note", "Keep the note to 500 characters or fewer."));
    }

    // Serialises payments for this customer so two cannot both pass the owes check.
    sqlx::query_scalar!(
        "SELECT id FROM customers WHERE id = $1 FOR UPDATE",
        customer_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    let owes = ledger::customer_owes(&mut tx, customer_id).await?;
    if owes <= Decimal::ZERO {
        return Err(AppError::nothing_owed());
    }
    if amount > owes {
        return Err(AppError::payment_over_owed(owes));
    }
    let open = ledger::customer_open_orders(&mut tx, customer_id).await?;
    let shares = allocate(amount, &open);
    let applied_total: Decimal = shares.iter().map(|(_, a)| *a).sum();
    if applied_total != amount {
        // owes counts orders that are not open (e.g. cancelled), so the open ones can't cover it.
        let open_total: Decimal = open.iter().map(|(_, a)| *a).sum();
        return Err(AppError::payment_over_owed(open_total));
    }

    let cash_account = if method == Method::Cash {
        Account::Cash
    } else {
        Account::Upi
    };
    let mut entries = vec![(cash_account, amount)];
    entries.extend(
        shares
            .iter()
            .map(|(o, share)| (Account::Receivable(*o), -*share)),
    );
    let transaction_id = ledger::post(
        &mut tx,
        TxnInput {
            kind: TxnKind::Payment,
            order_id: None,
            customer_id: Some(customer_id),
            method,
            actor,
            idempotency_key: format!("customer:{customer_id}:payment:{}", new_id()),
            note,
            reverses_id: None,
            entries,
        },
    )
    .await?;

    let ids: Vec<Uuid> = shares.iter().map(|(o, _)| *o).collect();
    let numbers: HashMap<Uuid, String> =
        sqlx::query!("SELECT id, number FROM orders WHERE id = ANY($1)", &ids)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| (r.id, r.number))
            .collect();
    let applied: Vec<Value> = shares
        .iter()
        .map(|(o, share)| {
            json!({ "order_id": o, "number": numbers.get(o), "amount": money(*share) })
        })
        .collect();
    let response = json!({
        "transaction_id": transaction_id,
        "applied": applied,
        "owes": money(owes - amount),
    });
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "customer.payment",
            entity_type: "customer",
            entity_id: Some(customer_id),
            before: None,
            after: Some(json!({
                "method": req.method,
                "amount": money(amount),
                "transaction_id": transaction_id,
                "applied": applied,
            })),
        },
    )
    .await?;
    idempotency::finish(&mut tx, actor, &scope, key, &response).await?;
    tx.commit().await?;
    Ok((true, response))
}

pub async fn payment_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<(StatusCode, Json<Value>)> {
    user.require("payments.record")?;
    let key = idempotency::key_from_headers(&headers)?;
    let (created, v) = record(&s.db, user.id, id, &key, body).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(v)))
}
