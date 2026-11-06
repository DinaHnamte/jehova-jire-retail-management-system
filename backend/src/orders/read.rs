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
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub product_code: Option<String>,
    pub quantity: i64,
    /// How many of `quantity` have been given back.
    pub returned_quantity: i64,
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
pub struct RefundRow {
    pub method: String,
    /// What was handed back in cash or UPI.
    pub amount: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReturnLineRow {
    pub order_line_id: Uuid,
    pub product_name: String,
    pub quantity: i64,
    pub disposition: String,
    pub amount: String,
}

#[derive(Debug, Serialize)]
pub struct ReturnRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub amount: String,
    pub owed_reduced: String,
    pub refunded: String,
    pub refund_method: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<ReturnLineRow>,
}

#[derive(Debug, Serialize)]
pub struct PaidByMethod {
    pub cash: String,
    pub upi: String,
}

/// The delivery behind a delivery order.
#[derive(Debug, Serialize)]
pub struct DeliveryRef {
    pub id: Uuid,
    pub status: String,
    pub driver_name: Option<String>,
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
    /// `counter` or `delivery`.
    pub fulfilment: String,
    /// The delivery fee as it stands now (`0.00` for counter sales).
    pub delivery_fee: String,
    /// Goods plus the current delivery fee.
    pub total: String,
    pub paid: String,
    pub owed: String,
    /// `paid`, `partly_paid`, `unpaid` or `cancelled`.
    pub payment_status: &'static str,
    /// What is currently paid per method: payments less refunds.
    pub paid_by_method: PaidByMethod,
    pub payments: Vec<PaymentRow>,
    /// Money handed back on this order (cancel or return), oldest first.
    pub refunds: Vec<RefundRow>,
    /// Returns taken on this sale, newest first.
    pub returns: Vec<ReturnRow>,
    pub note: Option<String>,
    pub receipt_url: String,
    pub whatsapp_url: Option<String>,
    pub delivery: Option<DeliveryRef>,
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
    pub fulfilment: String,
    pub delivery_status: Option<String>,
    pub total: String,
    pub paid: String,
    pub owed: String,
    pub payment_status: &'static str,
}

/// One sold line that has been given back, summed over all its returns.
#[derive(Debug, Serialize)]
pub struct ReturnedLine {
    pub name: String,
    pub code: Option<String>,
    pub quantity: i64,
    #[serde(serialize_with = "money_str")]
    pub amount: Decimal,
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
    /// The delivery fee as it stands now; zero for counter sales.
    #[serde(serialize_with = "money_str")]
    pub delivery_fee: Decimal,
    #[serde(serialize_with = "money_str")]
    pub total: Decimal,
    #[serde(serialize_with = "money_str")]
    pub paid: Decimal,
    #[serde(serialize_with = "money_str")]
    pub owed: Decimal,
    /// Total handed back in cash or UPI.
    #[serde(serialize_with = "money_str")]
    pub refunded: Decimal,
    pub returned: Vec<ReturnedLine>,
    pub cancelled: bool,
}

/// A cancelled sale is `cancelled`. Otherwise, with nothing owed it is `paid` (also for a 0 bill),
/// unless returns gave back all the goods (`total` here is the goods total: returns never refund
/// a delivery fee), which is `returned`. With something owed, nothing paid
/// (net of refunds) is `unpaid` and anything else is `partly_paid`. Returns shrink what is owed, so
/// the original total is never compared with what is owed.
pub fn payment_status(
    status: &str,
    total: Decimal,
    owed: Decimal,
    paid: Decimal,
    returned: Decimal,
) -> &'static str {
    if status == "cancelled" {
        "cancelled"
    } else if owed.is_zero() {
        if returned > Decimal::ZERO && returned == total {
            "returned"
        } else {
            "paid"
        }
    } else if paid.is_zero() {
        "unpaid"
    } else {
        "partly_paid"
    }
}

/// Sum of all returns taken on an order.
async fn returned_total(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Decimal> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(amount), 0) AS "n!" FROM returns WHERE order_id = $1"#,
        order_id
    )
    .fetch_one(conn)
    .await?)
}

pub(crate) async fn shop_name(conn: &mut PgConnection) -> AppResult<String> {
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
    fulfilment: String,
    /// Current: `deliveries.fee` for a delivery order.
    delivery_fee: Decimal,
    /// Current total: goods plus the current delivery fee.
    total: Decimal,
    goods_total: Decimal,
    note: Option<String>,
    receipt_token: String,
    created_at: DateTime<Utc>,
    delivery_id: Option<Uuid>,
    delivery_status: Option<String>,
    driver_name: Option<String>,
}

async fn order_row(conn: &mut PgConnection, id: Uuid) -> AppResult<OrderRow> {
    sqlx::query_as!(
        OrderRow,
        r#"SELECT o.id, o.number, o.status, o.customer_id, o.subtotal, o.order_discount,
                  o.fulfilment, COALESCE(d.fee, o.delivery_fee) AS "delivery_fee!",
                  o.total - o.delivery_fee + COALESCE(d.fee, 0) AS "total!",
                  o.total - o.delivery_fee AS "goods_total!", o.note, o.receipt_token, o.created_at,
                  d.id AS "delivery_id?", d.status AS "delivery_status?",
                  u.display_name AS "driver_name?"
           FROM orders o
           LEFT JOIN deliveries d ON d.order_id = o.id
           LEFT JOIN users u ON u.id = d.current_driver_id
           WHERE o.id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

struct LineRow {
    id: Uuid,
    returned_quantity: i64,
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
        r#"SELECT id, product_id, product_name, product_code, quantity,
                COALESCE((SELECT SUM(rl.quantity) FROM return_lines rl
                           WHERE rl.order_line_id = order_lines.id), 0)::bigint AS "returned_quantity!",
                list_price, unit_price, line_discount, order_discount_share, net_amount, unit_cost
         FROM order_lines WHERE order_id = $1 ORDER BY line_no"#,
        order_id
    )
    .fetch_all(conn)
    .await?)
}

/// What the order owes, what is currently paid and what was handed back.
#[derive(Clone, Copy)]
struct Money {
    owed: Decimal,
    paid: Decimal,
    refunded: Decimal,
}

fn receipt_from(
    shop_name: String,
    o: &OrderRow,
    lines: &[LineRow],
    customer_name: Option<String>,
    m: Money,
    returned: Vec<ReturnedLine>,
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
        delivery_fee: o.delivery_fee,
        total: o.total,
        paid: m.paid,
        owed: m.owed,
        refunded: m.refunded,
        returned,
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

/// Refunds on this order: each `refund` transaction's credit to cash or UPI.
async fn refund_rows(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<RefundRow>> {
    let rows = sqlx::query!(
        r#"SELECT t.id, COALESCE(t.method, 'cash') AS "method!",
                  -SUM(e.amount) AS "amount!", t.occurred_at
           FROM ledger_transactions t
           JOIN ledger_entries e
             ON e.transaction_id = t.id AND e.account IN ('cash', 'upi')
           WHERE t.order_id = $1 AND t.kind = 'refund'
           GROUP BY t.id, t.method, t.occurred_at
           ORDER BY t.occurred_at, t.id"#,
        order_id
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| RefundRow {
            method: r.method,
            amount: money(r.amount),
            occurred_at: r.occurred_at,
        })
        .collect())
}

/// Money in minus money out, per method: payments credited to this order less cash or UPI
/// handed back in its refunds. Returns (cash, upi, refunded).
pub(crate) async fn paid_by_method(
    conn: &mut PgConnection,
    order_id: Uuid,
) -> AppResult<(Decimal, Decimal, Decimal)> {
    let rows = sqlx::query!(
        r#"SELECT 'in' AS "dir!", COALESCE(t.method, 'cash') AS "method!", -SUM(e.amount) AS "amount!"
           FROM ledger_transactions t
           JOIN ledger_entries e
             ON e.transaction_id = t.id AND e.account = 'receivable' AND e.ref_id = $1
           WHERE t.kind = 'payment'
           GROUP BY t.method
           UNION ALL
           SELECT 'out', e.account, -SUM(e.amount)
           FROM ledger_transactions t
           JOIN ledger_entries e
             ON e.transaction_id = t.id AND e.account IN ('cash', 'upi')
           WHERE t.order_id = $1 AND t.kind = 'refund'
           GROUP BY e.account"#,
        order_id
    )
    .fetch_all(conn)
    .await?;
    let (mut cash, mut upi, mut refunded) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    for r in rows {
        let sign = if r.dir == "in" {
            Decimal::ONE
        } else {
            refunded += r.amount;
            -Decimal::ONE
        };
        match r.method.as_str() {
            "upi" => upi += sign * r.amount,
            _ => cash += sign * r.amount,
        }
    }
    Ok((cash, upi, refunded))
}

async fn return_rows(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<ReturnRow>> {
    let rows = sqlx::query!(
        "SELECT id, created_at, amount, owed_reduced, refunded, refund_method, note
         FROM returns WHERE order_id = $1 ORDER BY created_at DESC, id DESC",
        order_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let lines = sqlx::query!(
        r#"SELECT rl.return_id, rl.order_line_id, ol.product_name, rl.quantity, rl.disposition,
                  rl.amount
           FROM return_lines rl
           JOIN order_lines ol ON ol.id = rl.order_line_id
           JOIN returns r ON r.id = rl.return_id
           WHERE r.order_id = $1
           ORDER BY ol.line_no, rl.id"#,
        order_id
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ReturnRow {
            id: r.id,
            created_at: r.created_at,
            amount: money(r.amount),
            owed_reduced: money(r.owed_reduced),
            refunded: money(r.refunded),
            refund_method: r.refund_method,
            note: r.note,
            lines: lines
                .iter()
                .filter(|l| l.return_id == r.id)
                .map(|l| ReturnLineRow {
                    order_line_id: l.order_line_id,
                    product_name: l.product_name.clone(),
                    quantity: l.quantity,
                    disposition: l.disposition.clone(),
                    amount: money(l.amount),
                })
                .collect(),
        })
        .collect())
}

async fn returned_lines(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<ReturnedLine>> {
    let rows = sqlx::query!(
        r#"SELECT ol.product_name AS name, ol.product_code AS code,
                  SUM(rl.quantity)::bigint AS "quantity!", SUM(rl.amount) AS "amount!"
           FROM return_lines rl
           JOIN order_lines ol ON ol.id = rl.order_line_id
           WHERE ol.order_id = $1
           GROUP BY ol.id, ol.line_no, ol.product_name, ol.product_code
           ORDER BY ol.line_no"#,
        order_id
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ReturnedLine {
            name: r.name,
            code: r.code,
            quantity: r.quantity,
            amount: r.amount,
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
    let refunds = refund_rows(conn, id).await?;
    let returns = return_rows(conn, id).await?;
    let (cash, upi, refunded) = paid_by_method(conn, id).await?;
    let paid = cash + upi;
    let receipt_url = format!("{}/r/{}", cfg.public_url, o.receipt_token);
    let whatsapp_url = match &customer {
        Some(c) => {
            let m = Money {
                owed,
                paid,
                refunded,
            };
            let receipt = receipt_from(
                shop_name(conn).await?,
                &o,
                &lines,
                c.name.clone(),
                m,
                returned_lines(conn, id).await?,
            );
            let labels = |rows: &mut dyn Iterator<Item = &str>| -> Vec<&'static str> {
                let seen: Vec<&str> = rows.collect();
                [("cash", "cash"), ("upi", "UPI")]
                    .iter()
                    .filter(|(m, _)| seen.contains(m))
                    .map(|(_, label)| *label)
                    .collect()
            };
            let methods = labels(&mut payments.iter().map(|p| p.method.as_str()));
            let refund_methods = labels(&mut refunds.iter().map(|r| r.method.as_str()));
            Some(wa_link(
                &c.phone,
                &receipt_text(&receipt, &receipt_url, &methods, &refund_methods),
            ))
        }
        None => None,
    };
    let returned = returned_total(conn, id).await?;
    let payment_status = payment_status(&o.status, o.goods_total, owed, paid, returned);
    Ok(OrderDetail {
        id: o.id,
        number: o.number,
        status: o.status,
        created_at: o.created_at,
        customer,
        lines: lines
            .into_iter()
            .map(|l| LineOut {
                id: l.id,
                returned_quantity: l.returned_quantity,
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
        fulfilment: o.fulfilment,
        delivery_fee: money(o.delivery_fee),
        total: money(o.total),
        paid: money(paid),
        owed: money(owed),
        payment_status,
        paid_by_method: PaidByMethod {
            cash: money(cash),
            upi: money(upi),
        },
        payments,
        refunds,
        returns,
        note: o.note,
        receipt_url,
        whatsapp_url,
        delivery: match (o.delivery_id, o.delivery_status) {
            (Some(id), Some(status)) => Some(DeliveryRef {
                id,
                status,
                driver_name: o.driver_name,
            }),
            _ => None,
        },
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
    let (cash, upi, refunded) = paid_by_method(conn, id).await?;
    let returned = returned_lines(conn, id).await?;
    let m = Money {
        owed,
        paid: cash + upi,
        refunded,
    };
    Ok(receipt_from(
        shop_name(conn).await?,
        &o,
        &lines,
        customer_name,
        m,
        returned,
    ))
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaidFilter {
    Paid,
    PartlyPaid,
    Unpaid,
    /// Everything given back: nothing owed and returns equal the total.
    Returned,
}

impl PaidFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::PartlyPaid => "partly_paid",
            Self::Unpaid => "unpaid",
            Self::Returned => "returned",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusFilter {
    Confirmed,
    Cancelled,
}

impl StatusFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct OrderListParams {
    pub status: Option<StatusFilter>,
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
    let status = p.status.map(StatusFilter::as_str);
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM orders o
           LEFT JOIN deliveries d ON d.order_id = o.id
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = o.id) r
           CROSS JOIN LATERAL (SELECT COALESCE((SELECT -SUM(e.amount) FROM ledger_entries e
                                                 JOIN ledger_transactions t ON t.id = e.transaction_id
                                                WHERE t.kind = 'payment' AND e.account = 'receivable'
                                                  AND e.ref_id = o.id), 0)
                                    - COALESCE((SELECT -SUM(e.amount) FROM ledger_entries e
                                                 JOIN ledger_transactions t ON t.id = e.transaction_id
                                                WHERE t.kind = 'refund' AND t.order_id = o.id
                                                  AND e.account IN ('cash', 'upi')), 0) AS paid) pd
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(x.amount), 0) AS returned FROM returns x
                               WHERE x.order_id = o.id) rt
           WHERE ($1::timestamptz IS NULL OR o.created_at >= $1)
             AND ($2::timestamptz IS NULL OR o.created_at < $2)
             AND ($3::uuid IS NULL OR o.customer_id = $3)
             AND ($4::text IS NULL OR (o.status = 'confirmed'
                                       AND $4 = CASE WHEN r.owed = 0 AND rt.returned > 0
                                                          AND rt.returned = o.total - o.delivery_fee THEN 'returned'
                                                     WHEN r.owed = 0 THEN 'paid'
                                                     WHEN pd.paid = 0 THEN 'unpaid'
                                                     ELSE 'partly_paid' END))
             AND ($5::text IS NULL OR o.status = $5)"#,
        from,
        until,
        p.customer_id,
        paid,
        status
    )
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT o.id, o.number, o.status, o.created_at, o.customer_id,
                  o.total - o.delivery_fee + COALESCE(d.fee, 0) AS "total!",
                  o.total - o.delivery_fee AS "goods_total!", o.fulfilment,
                  d.status AS "delivery_status?",
                  c.name AS "customer_name?", c.phone AS "customer_phone?", r.owed AS "owed!",
                  pd.paid AS "paid!", rt.returned AS "returned!"
           FROM orders o
           LEFT JOIN deliveries d ON d.order_id = o.id
           LEFT JOIN customers c ON c.id = o.customer_id
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = o.id) r
           CROSS JOIN LATERAL (SELECT COALESCE((SELECT -SUM(e.amount) FROM ledger_entries e
                                                 JOIN ledger_transactions t ON t.id = e.transaction_id
                                                WHERE t.kind = 'payment' AND e.account = 'receivable'
                                                  AND e.ref_id = o.id), 0)
                                    - COALESCE((SELECT -SUM(e.amount) FROM ledger_entries e
                                                 JOIN ledger_transactions t ON t.id = e.transaction_id
                                                WHERE t.kind = 'refund' AND t.order_id = o.id
                                                  AND e.account IN ('cash', 'upi')), 0) AS paid) pd
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(x.amount), 0) AS returned FROM returns x
                               WHERE x.order_id = o.id) rt
           WHERE ($1::timestamptz IS NULL OR o.created_at >= $1)
             AND ($2::timestamptz IS NULL OR o.created_at < $2)
             AND ($3::uuid IS NULL OR o.customer_id = $3)
             AND ($4::text IS NULL OR (o.status = 'confirmed'
                                       AND $4 = CASE WHEN r.owed = 0 AND rt.returned > 0
                                                          AND rt.returned = o.total - o.delivery_fee THEN 'returned'
                                                     WHEN r.owed = 0 THEN 'paid'
                                                     WHEN pd.paid = 0 THEN 'unpaid'
                                                     ELSE 'partly_paid' END))
             AND ($5::text IS NULL OR o.status = $5)
           ORDER BY o.created_at DESC, o.id DESC
           LIMIT $6 OFFSET $7"#,
        from,
        until,
        p.customer_id,
        paid,
        status,
        paging.per_page,
        paging.offset()
    )
    .fetch_all(&mut *conn)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| OrderSummary {
            payment_status: payment_status(&r.status, r.goods_total, r.owed, r.paid, r.returned),
            id: r.id,
            number: r.number,
            status: r.status,
            created_at: r.created_at,
            customer_id: r.customer_id,
            customer_name: r.customer_name,
            customer_phone: r.customer_phone,
            fulfilment: r.fulfilment,
            delivery_status: r.delivery_status,
            paid: money(r.paid),
            owed: money(r.owed),
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
