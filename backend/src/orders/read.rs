//! Order reads: detail, list and the public receipt page (spec §3, §5).
use axum::{Json, extract::State};
use chrono::{DateTime, Days, NaiveDate, NaiveTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};
use sqlx::PgConnection;
use uuid::Uuid;

use super::whatsapp::{receipt_text, wa_link};
use crate::{
    AppState,
    admin::session::CurrentUser,
    config::Config,
    customers::{self, dto::Customer},
    error::{AppError, AppResult},
    http::{auth::ClientMeta, path::ApiPath, query::ApiQuery},
    inventory::ist,
    ledger,
    shared::paging::{Page, Paging},
};

const DEFAULT_SHOP: &str = "Jehovah Jireh";

fn money_str<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{:.2}", d.round_dp(2)))
}

fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

#[derive(Debug, Serialize)]
pub struct LineOut {
    pub product_id: Uuid,
    pub product_name: String,
    pub product_code: Option<String>,
    pub quantity: i64,
    pub list_price: String,
    pub unit_price: String,
    pub line_discount: String,
    pub order_discount_share: String,
    /// After this line's share of the order discount.
    pub net_amount: String,
    /// Only for users who can update products.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaymentRow {
    pub id: Uuid,
    pub method: String,
    /// What this payment credited to the order (a share, when one payment covered several).
    pub amount: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct OrderDetail {
    pub id: Uuid,
    pub number: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub customer: Option<Customer>,
    pub lines: Vec<LineOut>,
    pub subtotal: String,
    pub order_discount: String,
    pub total: String,
    pub paid: String,
    pub owed: String,
    /// `paid`, `partly_paid` or `unpaid`.
    pub payment_status: &'static str,
    pub payments: Vec<PaymentRow>,
    pub note: Option<String>,
    pub receipt_url: String,
    pub whatsapp_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OrderSummary {
    pub id: Uuid,
    pub number: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub customer_id: Option<Uuid>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub total: String,
    pub paid: String,
    pub owed: String,
    pub payment_status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReceiptLine {
    pub name: String,
    pub code: Option<String>,
    pub quantity: i64,
    #[serde(serialize_with = "money_str")]
    pub unit_price: Decimal,
    #[serde(serialize_with = "money_str")]
    pub list_price: Decimal,
    /// Before the order discount; rows sum to `subtotal`.
    #[serde(serialize_with = "money_str")]
    pub net_amount: Decimal,
}

/// What the customer sees on the public receipt page. Never carries cost.
#[derive(Debug, Serialize)]
pub struct PublicReceipt {
    pub shop_name: String,
    pub number: String,
    pub created_at: DateTime<Utc>,
    pub customer_name: Option<String>,
    pub lines: Vec<ReceiptLine>,
    #[serde(serialize_with = "money_str")]
    pub subtotal: Decimal,
    #[serde(serialize_with = "money_str")]
    pub order_discount: Decimal,
    #[serde(serialize_with = "money_str")]
    pub total: Decimal,
    #[serde(serialize_with = "money_str")]
    pub paid: Decimal,
    #[serde(serialize_with = "money_str")]
    pub owed: Decimal,
    pub cancelled: bool,
}

/// `owed == 0` is paid (also for a 0 bill); `owed == total` is unpaid; anything between is partly.
pub fn payment_status(total: Decimal, owed: Decimal) -> &'static str {
    if owed.is_zero() {
        "paid"
    } else if owed == total {
        "unpaid"
    } else {
        "partly_paid"
    }
}

async fn shop_name(conn: &mut PgConnection) -> AppResult<String> {
    let name: Option<String> =
        sqlx::query_scalar!(r#"SELECT value #>> '{}' FROM settings WHERE key = 'shop_name'"#)
            .fetch_optional(conn)
            .await?
            .flatten();
    Ok(name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SHOP.to_string()))
}

struct OrderRow {
    id: Uuid,
    number: String,
    status: String,
    customer_id: Option<Uuid>,
    subtotal: Decimal,
    order_discount: Decimal,
    total: Decimal,
    note: Option<String>,
    receipt_token: String,
    created_at: DateTime<Utc>,
}

async fn order_row(conn: &mut PgConnection, id: Uuid) -> AppResult<OrderRow> {
    sqlx::query_as!(
        OrderRow,
        "SELECT id, number, status, customer_id, subtotal, order_discount, total, note,
                receipt_token, created_at
         FROM orders WHERE id = $1",
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

struct LineRow {
    product_id: Uuid,
    product_name: String,
    product_code: Option<String>,
    quantity: i64,
    list_price: Decimal,
    unit_price: Decimal,
    line_discount: Decimal,
    order_discount_share: Decimal,
    net_amount: Decimal,
    unit_cost: Option<Decimal>,
}

async fn line_rows(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<LineRow>> {
    Ok(sqlx::query_as!(
        LineRow,
        "SELECT product_id, product_name, product_code, quantity, list_price, unit_price,
                line_discount, order_discount_share, net_amount, unit_cost
         FROM order_lines WHERE order_id = $1 ORDER BY line_no",
        order_id
    )
    .fetch_all(conn)
    .await?)
}

fn receipt_from(
    shop_name: String,
    o: &OrderRow,
    lines: &[LineRow],
    customer_name: Option<String>,
    owed: Decimal,
) -> PublicReceipt {
    PublicReceipt {
        shop_name,
        number: o.number.clone(),
        created_at: o.created_at,
        customer_name,
        lines: lines
            .iter()
            .map(|l| ReceiptLine {
                name: l.product_name.clone(),
                code: l.product_code.clone(),
                quantity: l.quantity,
                unit_price: l.unit_price,
                list_price: l.list_price,
                net_amount: l.net_amount + l.order_discount_share,
            })
            .collect(),
        subtotal: o.subtotal,
        order_discount: o.order_discount,
        total: o.total,
        paid: o.total - owed,
        owed,
        cancelled: o.status == "cancelled",
    }
}

/// Payments on this order: each payment transaction's credit to this order's receivable.
async fn payment_rows(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<PaymentRow>> {
    let rows = sqlx::query!(
        r#"SELECT t.id, COALESCE(t.method, 'cash') AS "method!",
                  -SUM(e.amount) AS "amount!", t.occurred_at
           FROM ledger_transactions t
           JOIN ledger_entries e
             ON e.transaction_id = t.id AND e.account = 'receivable' AND e.ref_id = $1
           WHERE t.kind = 'payment'
           GROUP BY t.id, t.method, t.occurred_at
           ORDER BY t.occurred_at, t.id"#,
        order_id
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PaymentRow {
            id: r.id,
            method: r.method,
            amount: money(r.amount),
            occurred_at: r.occurred_at,
        })
        .collect())
}

pub async fn order_detail(
    conn: &mut PgConnection,
    id: Uuid,
    show_cost: bool,
    cfg: &Config,
) -> AppResult<OrderDetail> {
    let o = order_row(conn, id).await?;
    let lines = line_rows(conn, id).await?;
    let customer = match o.customer_id {
        Some(cid) => Some(customers::repo::fetch(conn, cid).await?),
        None => None,
    };
    let owed = ledger::order_owed(conn, id).await?;
    let payments = payment_rows(conn, id).await?;
    let receipt_url = format!("{}/r/{}", cfg.public_url, o.receipt_token);
    let whatsapp_url = match &customer {
        Some(c) => {
            let receipt = receipt_from(shop_name(conn).await?, &o, &lines, c.name.clone(), owed);
            let methods: Vec<&str> = [("cash", "cash"), ("upi", "UPI")]
                .iter()
                .filter(|(m, _)| payments.iter().any(|p| p.method == *m))
                .map(|(_, label)| *label)
                .collect();
            Some(wa_link(
                &c.phone,
                &receipt_text(&receipt, &receipt_url, &methods),
            ))
        }
        None => None,
    };
    Ok(OrderDetail {
        id: o.id,
        number: o.number,
        status: o.status,
        created_at: o.created_at,
        customer,
        lines: lines
            .into_iter()
            .map(|l| LineOut {
                product_id: l.product_id,
                product_name: l.product_name,
                product_code: l.product_code,
                quantity: l.quantity,
                list_price: money(l.list_price),
                unit_price: money(l.unit_price),
                line_discount: money(l.line_discount),
                order_discount_share: money(l.order_discount_share),
                net_amount: money(l.net_amount),
                unit_cost: l
                    .unit_cost
                    .filter(|_| show_cost)
                    .map(|c| format!("{:.4}", c.round_dp(4))),
            })
            .collect(),
        subtotal: money(o.subtotal),
        order_discount: money(o.order_discount),
        total: money(o.total),
        paid: money(o.total - owed),
        owed: money(owed),
        payment_status: payment_status(o.total, owed),
        payments,
        note: o.note,
        receipt_url,
        whatsapp_url,
    })
}

pub async fn public_receipt(conn: &mut PgConnection, token: &str) -> AppResult<PublicReceipt> {
    let id = sqlx::query_scalar!("SELECT id FROM orders WHERE receipt_token = $1", token)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    let o = order_row(conn, id).await?;
    let lines = line_rows(conn, id).await?;
    let customer_name = match o.customer_id {
        Some(cid) => customers::repo::fetch(conn, cid).await?.name,
        None => None,
    };
    let owed = ledger::order_owed(conn, id).await?;
    Ok(receipt_from(
        shop_name(conn).await?,
        &o,
        &lines,
        customer_name,
        owed,
    ))
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaidFilter {
    Paid,
    PartlyPaid,
    Unpaid,
}

impl PaidFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::PartlyPaid => "partly_paid",
            Self::Unpaid => "unpaid",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct OrderListParams {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub paid: Option<PaidFilter>,
    pub customer_id: Option<Uuid>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Midnight at the start of `d` in IST.
fn ist_start(d: NaiveDate) -> DateTime<Utc> {
    ist()
        .from_local_datetime(&d.and_time(NaiveTime::MIN))
        .single()
        .expect("fixed offset has no gaps")
        .with_timezone(&Utc)
}

/// Orders newest first. `from` and `to` are IST calendar dates, both inclusive.
pub async fn list(conn: &mut PgConnection, p: OrderListParams) -> AppResult<Page<OrderSummary>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let from = p.from.map(ist_start);
    let until =
        p.to.map(|d| ist_start(d.checked_add_days(Days::new(1)).unwrap_or(d)));
    let paid = p.paid.map(PaidFilter::as_str);
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM orders o
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = o.id) r
           WHERE ($1::timestamptz IS NULL OR o.created_at >= $1)
             AND ($2::timestamptz IS NULL OR o.created_at < $2)
             AND ($3::uuid IS NULL OR o.customer_id = $3)
             AND ($4::text IS NULL OR $4 = CASE WHEN r.owed = 0 THEN 'paid'
                                                WHEN r.owed = o.total THEN 'unpaid'
                                                ELSE 'partly_paid' END)"#,
        from,
        until,
        p.customer_id,
        paid
    )
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT o.id, o.number, o.status, o.created_at, o.customer_id, o.total,
                  c.name AS "customer_name?", c.phone AS "customer_phone?", r.owed AS "owed!"
           FROM orders o
           LEFT JOIN customers c ON c.id = o.customer_id
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = o.id) r
           WHERE ($1::timestamptz IS NULL OR o.created_at >= $1)
             AND ($2::timestamptz IS NULL OR o.created_at < $2)
             AND ($3::uuid IS NULL OR o.customer_id = $3)
             AND ($4::text IS NULL OR $4 = CASE WHEN r.owed = 0 THEN 'paid'
                                                WHEN r.owed = o.total THEN 'unpaid'
                                                ELSE 'partly_paid' END)
           ORDER BY o.created_at DESC, o.id DESC
           LIMIT $5 OFFSET $6"#,
        from,
        until,
        p.customer_id,
        paid,
        paging.per_page,
        paging.offset()
    )
    .fetch_all(&mut *conn)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| OrderSummary {
            id: r.id,
            number: r.number,
            status: r.status,
            created_at: r.created_at,
            customer_id: r.customer_id,
            customer_name: r.customer_name,
            customer_phone: r.customer_phone,
            paid: money(r.total - r.owed),
            owed: money(r.owed),
            payment_status: payment_status(r.total, r.owed),
            total: money(r.total),
        })
        .collect();
    Ok(Page {
        items,
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}

pub async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<OrderDetail>> {
    user.require("orders.read")?;
    let mut conn = s.db.acquire().await?;
    let show_cost = user.can("products.update");
    Ok(Json(
        order_detail(&mut conn, id, show_cost, &s.config).await?,
    ))
}

pub async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<OrderListParams>,
) -> AppResult<Json<Page<OrderSummary>>> {
    user.require("orders.read")?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(list(&mut conn, p).await?))
}

/// Public receipt page data. No login; limited per client IP.
pub async fn public_receipt_route(
    State(s): State<AppState>,
    meta: ClientMeta,
    ApiPath(token): ApiPath<String>,
) -> AppResult<Json<PublicReceipt>> {
    if !s
        .receipt_limiter
        .check(meta.ip.as_deref().unwrap_or("anon"))
    {
        return Err(AppError::too_many_requests());
    }
    let mut conn = s.db.acquire().await?;
    Ok(Json(public_receipt(&mut conn, &token).await?))
}
