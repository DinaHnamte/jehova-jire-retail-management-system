//! One-call checkout (spec §3): a numbered sale that takes stock off, posts the money to the
//! ledger and records any payment, all in one transaction and safe to retry.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use super::{
    domain::{LineIn, PriceError, price_lines},
    numbers::next_number,
    read,
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit, customers,
    error::{AppError, AppResult},
    http::json::ApiJson,
    inventory::{
        check_line_products,
        domain::{MovementType, SourceType},
        ledger::{self as stock, MovementInput, Source},
        line_field,
    },
    ledger::{self, Account, Method, TxnInput, TxnKind},
    shared::{
        idempotency::{self, Begin},
        ids::new_id,
        money::{MAX_MONEY, format_rupees, parse_money},
    },
};

const SCOPE: &str = "checkout";
const MAX_QTY: i64 = 1_000_000;

#[derive(Deserialize)]
pub struct CheckoutRequest {
    #[serde(default)]
    pub lines: Vec<LineRequest>,
    pub order_discount: Option<String>,
    pub customer_id: Option<Uuid>,
    pub new_customer: Option<NewCustomer>,
    #[serde(default)]
    pub payments: Vec<PaymentRequest>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct LineRequest {
    pub product_id: Uuid,
    pub quantity: i64,
    /// Defaults to the product's selling price.
    pub unit_price: Option<String>,
    pub line_discount: Option<String>,
}

#[derive(Deserialize)]
pub struct NewCustomer {
    pub phone: String,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct PaymentRequest {
    pub method: String,
    pub amount: String,
}

fn field(name: impl Into<String>, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: name.into(),
        message: message.into(),
    }
}

struct ValidLine {
    product_id: Uuid,
    quantity: i64,
    unit_price: Option<Decimal>,
    line_discount: Decimal,
}

struct Valid {
    lines: Vec<ValidLine>,
    order_discount: Decimal,
    payments: Vec<(Method, Decimal)>,
    note: Option<String>,
}

/// Checks every input before any pricing, so the maths never sees a negative or
/// over-precise amount.
fn validate(req: &CheckoutRequest) -> AppResult<Valid> {
    if req.lines.is_empty() {
        return Err(field("lines", "Add at least one item."));
    }
    let mut lines = Vec::with_capacity(req.lines.len());
    for (i, l) in req.lines.iter().enumerate() {
        if !(1..=MAX_QTY).contains(&l.quantity) {
            return Err(line_field(
                i,
                "quantity",
                "Enter a quantity from 1 to 1,000,000.",
            ));
        }
        let unit_price = l
            .unit_price
            .as_deref()
            .map(parse_money)
            .transpose()
            .map_err(|m| line_field(i, "unit_price", m))?;
        let line_discount = l
            .line_discount
            .as_deref()
            .map(parse_money)
            .transpose()
            .map_err(|m| line_field(i, "line_discount", m))?
            .unwrap_or(Decimal::ZERO);
        lines.push(ValidLine {
            product_id: l.product_id,
            quantity: l.quantity,
            unit_price,
            line_discount,
        });
    }
    let order_discount = req
        .order_discount
        .as_deref()
        .map(parse_money)
        .transpose()
        .map_err(|m| field("order_discount", m))?
        .unwrap_or(Decimal::ZERO);

    let mut payments: Vec<(Method, Decimal)> = Vec::with_capacity(req.payments.len());
    for (i, p) in req.payments.iter().enumerate() {
        let method = match p.method.as_str() {
            "cash" => Method::Cash,
            "upi" => Method::Upi,
            _ => return Err(field(format!("payments[{i}].method"), "Pick cash or UPI.")),
        };
        if payments.iter().any(|(m, _)| *m == method) {
            return Err(field(
                format!("payments[{i}].method"),
                "Enter cash and UPI once each.",
            ));
        }
        let amount =
            parse_money(&p.amount).map_err(|m| field(format!("payments[{i}].amount"), m))?;
        if amount.is_zero() {
            return Err(field(
                format!("payments[{i}].amount"),
                "Enter an amount more than 0.",
            ));
        }
        payments.push((method, amount));
    }

    if req.customer_id.is_some() && req.new_customer.is_some() {
        return Err(field(
            "customer_id",
            "Pick a saved customer or add a new one, not both.",
        ));
    }
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
        order_discount,
        payments,
        note,
    })
}

struct ProductSnap {
    name: String,
    code: Option<String>,
    selling_price: Decimal,
}

async fn product_snaps(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> AppResult<HashMap<Uuid, ProductSnap>> {
    let rows = sqlx::query!(
        r#"SELECT p.id, p.name, p.selling_price, pi.raw_value AS "code?"
           FROM products p
           LEFT JOIN product_identifiers pi
                  ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
           WHERE p.id = ANY($1)"#,
        ids
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                ProductSnap {
                    name: r.name,
                    code: r.code,
                    selling_price: r.selling_price,
                },
            )
        })
        .collect())
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

fn receipt_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random number generator available");
    URL_SAFE_NO_PAD.encode(buf)
}

/// Runs a checkout. Returns `(created, response)`: `created` is false for a replay.
pub async fn checkout(
    db: &PgPool,
    actor: Uuid,
    key: &str,
    body: Value,
) -> AppResult<(bool, Value)> {
    let hash = idempotency::hash_body(&body);
    let mut tx = db.begin().await?;
    if let Begin::Replay(v) = idempotency::begin(&mut tx, actor, SCOPE, key, &hash).await? {
        return Ok((false, v));
    }
    let req: CheckoutRequest =
        serde_json::from_value(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let v = validate(&req)?;

    // Products and prices.
    let ids: Vec<Uuid> = v.lines.iter().map(|l| l.product_id).collect();
    check_line_products(&mut tx, &ids, "sale").await?;
    let snaps = product_snaps(&mut tx, &ids).await?;
    let line_ins: Vec<LineIn> = v
        .lines
        .iter()
        .map(|l| {
            let list = snaps[&l.product_id].selling_price;
            LineIn {
                quantity: l.quantity,
                list_price: list,
                unit_price: l.unit_price.unwrap_or(list),
                line_discount: l.line_discount,
            }
        })
        .collect();
    let (priced, subtotal, total) =
        price_lines(&line_ins, v.order_discount).map_err(|e| match e {
            PriceError::LineDiscountTooBig(i) => {
                let gross = line_ins[i].unit_price * Decimal::from(line_ins[i].quantity);
                line_field(
                    i,
                    "line_discount",
                    format!(
                        "The discount is more than this line. Enter {} or less.",
                        format_rupees(gross)
                    ),
                )
            }
            PriceError::OrderDiscountTooBig { max } => field(
                "order_discount",
                format!(
                    "The discount is more than the bill. Enter {} or less.",
                    format_rupees(max)
                ),
            ),
        })?;
    let max_money = Decimal::from_str(MAX_MONEY).expect("valid constant");
    if let Some(i) = priced.iter().position(|p| p.gross > max_money) {
        return Err(line_field(
            i,
            "quantity",
            "This line comes to more than ₹99,99,999.99. Lower the quantity or split the sale.",
        ));
    }
    if subtotal > max_money {
        return Err(field(
            "lines",
            "The bill comes to more than ₹99,99,999.99. Split it into two sales.",
        ));
    }

    // Payments against the total.
    let paid: Decimal = v.payments.iter().map(|(_, a)| *a).sum();
    if paid > total {
        return Err(AppError::overpayment(total));
    }
    if paid < total && req.customer_id.is_none() && req.new_customer.is_none() {
        return Err(AppError::customer_required());
    }

    // Customer.
    let customer_id = if let Some(id) = req.customer_id {
        match customers::repo::fetch(&mut tx, id).await {
            Ok(c) => Some(c.id),
            Err(AppError::NotFound) => {
                return Err(field("customer_id", "Pick a saved customer from the list."));
            }
            Err(e) => return Err(e),
        }
    } else if let Some(nc) = &req.new_customer {
        Some(customers::find_or_create(&mut tx, actor, &nc.phone, nc.name.as_deref()).await?)
    } else {
        None
    };

    // Number and order row.
    let order_id = new_id();
    // The database clock at transaction start: the same value the ledger rows default to, so
    // the order and its money always fall on the same day (and the same fiscal year).
    let now = sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
        .fetch_one(&mut *tx)
        .await?;
    let number = next_number(&mut tx, "receipt", now).await?;
    sqlx::query!(
        "INSERT INTO orders (id, number, customer_id, status, subtotal, order_discount, total,
                             note, receipt_token, created_by, created_at)
         VALUES ($1, $2, $3, 'confirmed', $4, $5, $6, $7, $8, $9, $10)",
        order_id,
        number,
        customer_id,
        subtotal,
        v.order_discount,
        total,
        v.note,
        receipt_token(),
        actor,
        now
    )
    .execute(&mut *tx)
    .await?;

    // Stock out, then lines with the cost of what left.
    let moves: Vec<MovementInput> = v
        .lines
        .iter()
        .map(|l| MovementInput {
            product_id: l.product_id,
            movement_type: MovementType::Sale,
            quantity: l.quantity,
            unit_cost: None,
            reason: None,
        })
        .collect();
    let balances = stock::apply(
        &mut tx,
        actor,
        Source {
            source_type: SourceType::Order,
            source_id: order_id,
        },
        &moves,
    )
    .await?;
    let avg: HashMap<Uuid, Option<Decimal>> = balances
        .into_iter()
        .map(|b| (b.product_id, b.avg_cost))
        .collect();
    for (i, (l, p)) in line_ins.iter().zip(&priced).enumerate() {
        let product_id = v.lines[i].product_id;
        let snap = &snaps[&product_id];
        sqlx::query!(
            "INSERT INTO order_lines
               (id, order_id, line_no, product_id, product_name, product_code, quantity,
                list_price, unit_price, line_discount, order_discount_share, net_amount, unit_cost)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            new_id(),
            order_id,
            i as i32 + 1,
            product_id,
            snap.name,
            snap.code,
            l.quantity,
            l.list_price,
            l.unit_price,
            p.line_discount,
            p.order_discount_share,
            p.net_amount,
            avg.get(&product_id).copied().flatten()
        )
        .execute(&mut *tx)
        .await?;
    }

    // Money: the sale makes a receivable; each payment clears part of it.
    if !total.is_zero() {
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::Sale,
                order_id: Some(order_id),
                customer_id,
                method: Method::Internal,
                actor,
                idempotency_key: format!("order:{order_id}:sale"),
                note: None,
                reverses_id: None,
                entries: vec![
                    (Account::Receivable(order_id), total),
                    (Account::SalesRevenue, -total),
                ],
            },
        )
        .await?;
    }
    let split = v.payments.len() > 1;
    for (method, amount) in &v.payments {
        let (account, suffix) = match method {
            Method::Cash => (Account::Cash, ":cash"),
            _ => (Account::Upi, ":upi"),
        };
        ledger::post(
            &mut tx,
            TxnInput {
                kind: TxnKind::Payment,
                order_id: Some(order_id),
                customer_id,
                method: *method,
                actor,
                idempotency_key: format!(
                    "order:{order_id}:payment{}",
                    if split { suffix } else { "" }
                ),
                note: None,
                reverses_id: None,
                entries: vec![
                    (account, *amount),
                    (Account::Receivable(order_id), -*amount),
                ],
            },
        )
        .await?;
    }

    let response = json!({
        "id": order_id,
        "number": number,
        "total": money(total),
        "paid": money(paid),
        "owed": money(total - paid),
        "customer_id": customer_id,
    });
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "order.checkout",
            entity_type: "order",
            entity_id: Some(order_id),
            before: None,
            after: Some(response.clone()),
        },
    )
    .await?;
    idempotency::finish(&mut tx, actor, SCOPE, key, &response).await?;
    tx.commit().await?;
    Ok((true, response))
}

pub async fn checkout_route(
    State(s): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    ApiJson(body): ApiJson<Value>,
) -> AppResult<(StatusCode, Json<read::OrderDetail>)> {
    user.require("orders.confirm")?;
    let key = idempotency::key_from_headers(&headers)?;
    let (created, v) = checkout(&s.db, user.id, &key, body).await?;
    // The stored response is used only for its order id; the detail is rebuilt so cost
    // visibility follows the caller.
    let id = v["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("stored checkout has no order id")))?;
    let mut conn = s.db.acquire().await?;
    let detail = read::order_detail(&mut conn, id, user.can("products.update"), &s.config).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(detail)))
}
