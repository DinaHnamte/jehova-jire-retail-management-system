//! Delivery reads for the owner: the tabbed list and the detail (spec §4, §5).
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::messages::{self, Job};
use crate::{
    AppState,
    admin::session::CurrentUser,
    config::Config,
    error::{AppError, AppResult},
    http::{path::ApiPath, query::ApiQuery},
    ledger,
    orders::{read as orders_read, whatsapp::wa_link},
};

pub(crate) fn money(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

/// What the driver collects at the door: what is owed when the driver collects, else nothing.
fn to_collect(collector: &str, owed: Decimal) -> Decimal {
    if collector == "driver" {
        owed.max(Decimal::ZERO)
    } else {
        Decimal::ZERO
    }
}

#[derive(Debug, Serialize)]
pub struct DriverBrief {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct ListItem {
    pub id: Uuid,
    pub order_id: Uuid,
    pub order_number: String,
    pub status: String,
    pub recipient_name: String,
    pub locality: String,
    pub driver: Option<DriverBrief>,
    pub amount_to_collect: String,
    pub fee: String,
    /// When the status last changed.
    pub since: DateTime<Utc>,
    /// The latest attempt's reason, for a failed delivery.
    pub fail_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Counts {
    pub to_send: i64,
    pub out: i64,
    pub failed: i64,
}

#[derive(Debug, Serialize)]
pub struct DeliveryList {
    pub items: Vec<ListItem>,
    pub counts: Counts,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    #[default]
    ToSend,
    /// Assigned or picked up.
    Out,
    Failed,
    /// Delivered, returned or cancelled in the last 7 days.
    Done,
}

impl Tab {
    fn statuses(self) -> Vec<String> {
        let s: &[&str] = match self {
            Self::ToSend => &["to_send"],
            Self::Out => &["assigned", "picked_up"],
            Self::Failed => &["failed"],
            Self::Done => &["delivered", "returned", "cancelled"],
        };
        s.iter().map(|s| s.to_string()).collect()
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub status: Option<Tab>,
}

/// One tab, oldest status change first (Done: newest first), with the counts of the open tabs.
pub async fn list(conn: &mut PgConnection, tab: Tab) -> AppResult<DeliveryList> {
    let done = tab == Tab::Done;
    let rows = sqlx::query!(
        r#"SELECT d.id, d.order_id, o.number AS order_number, d.status, d.recipient_name,
                  d.locality, d.collector, d.fee, d.current_driver_id,
                  u.display_name AS "driver_name?", u.is_active AS "driver_active?",
                  r.owed AS "owed!", s.since AS "since!",
                  (SELECT a.reason FROM delivery_attempts a WHERE a.delivery_id = d.id
                    ORDER BY a.occurred_at DESC, a.id DESC LIMIT 1) AS fail_reason
           FROM deliveries d
           JOIN orders o ON o.id = d.order_id
           LEFT JOIN users u ON u.id = d.current_driver_id
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = d.order_id) r
           CROSS JOIN LATERAL (SELECT COALESCE(MAX(ev.occurred_at), d.created_at) AS since
                               FROM delivery_events ev
                               WHERE ev.delivery_id = d.id AND ev.kind = 'status') s
           WHERE d.status = ANY($1)
             AND (NOT $2 OR COALESCE(d.closed_at, d.delivered_at) >= now() - interval '7 days')
           ORDER BY CASE WHEN $2 THEN s.since END DESC, s.since ASC, d.id"#,
        &tab.statuses(),
        done
    )
    .fetch_all(&mut *conn)
    .await?;
    let counts = sqlx::query_as!(
        Counts,
        r#"SELECT count(*) FILTER (WHERE status = 'to_send') AS "to_send!",
                  count(*) FILTER (WHERE status IN ('assigned', 'picked_up')) AS "out!",
                  count(*) FILTER (WHERE status = 'failed') AS "failed!"
           FROM deliveries"#
    )
    .fetch_one(&mut *conn)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| ListItem {
            driver: match (r.current_driver_id, r.driver_name, r.driver_active) {
                (Some(id), Some(name), Some(active)) => Some(DriverBrief { id, name, active }),
                _ => None,
            },
            fail_reason: r.fail_reason.filter(|_| r.status == "failed"),
            amount_to_collect: money(to_collect(&r.collector, r.owed)),
            id: r.id,
            order_id: r.order_id,
            order_number: r.order_number,
            status: r.status,
            recipient_name: r.recipient_name,
            locality: r.locality,
            fee: money(r.fee),
            since: r.since,
        })
        .collect();
    Ok(DeliveryList { items, counts })
}

#[derive(Debug, Serialize)]
pub struct OrderMoney {
    pub id: Uuid,
    pub number: String,
    /// The current total (Ruling 1): the goods plus the delivery's current fee.
    pub total: String,
    pub paid: String,
    pub owed: String,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub name: String,
    pub code: Option<String>,
    pub quantity: i64,
}

#[derive(Debug, Serialize)]
pub struct DriverOut {
    pub id: Uuid,
    pub name: String,
    pub phone: String,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct EventOut {
    pub from: Option<String>,
    pub to: String,
    pub kind: String,
    pub actor_name: String,
    pub note: Option<String>,
    /// For an edit: `{field: [old, new]}`.
    pub changes: Option<Value>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AttemptOut {
    pub driver_name: String,
    pub reason: String,
    pub note: Option<String>,
    pub trip_pay: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DeliveryDetail {
    pub id: Uuid,
    pub order_id: Uuid,
    pub status: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub address_line: String,
    pub locality: String,
    pub landmark: Option<String>,
    pub instructions: Option<String>,
    pub fee: String,
    pub driver_pay: Option<String>,
    pub collector: String,
    pub pay_method: String,
    pub received_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub picked_up_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub order: OrderMoney,
    pub items: Vec<Item>,
    pub driver: Option<DriverOut>,
    /// Oldest first.
    pub events: Vec<EventOut>,
    /// Oldest first.
    pub attempts: Vec<AttemptOut>,
    pub amount_to_collect: String,
    /// Both null when no driver has the job.
    pub driver_whatsapp_url: Option<String>,
    pub customer_whatsapp_url: Option<String>,
    pub maps_url: String,
}

pub async fn delivery_detail(
    conn: &mut PgConnection,
    id: Uuid,
    cfg: &Config,
) -> AppResult<DeliveryDetail> {
    let d = sqlx::query!(
        r#"SELECT d.id, d.order_id, d.status, d.recipient_name, d.recipient_phone, d.address_line,
                  d.locality, d.landmark, d.instructions, d.fee, d.driver_pay, d.collector,
                  d.pay_method, d.received_by, d.created_at, d.assigned_at, d.picked_up_at,
                  d.delivered_at, d.closed_at, d.updated_at, d.current_driver_id,
                  o.number, (o.total - o.delivery_fee + d.fee) AS "total!",
                  u.display_name AS "driver_name?", u.is_active AS "driver_active?",
                  p.phone AS "driver_phone?"
           FROM deliveries d
           JOIN orders o ON o.id = d.order_id
           LEFT JOIN users u ON u.id = d.current_driver_id
           LEFT JOIN driver_profiles p ON p.user_id = d.current_driver_id
           WHERE d.id = $1"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let items = sqlx::query_as!(
        Item,
        r#"SELECT product_name AS name, product_code AS code, quantity
           FROM order_lines WHERE order_id = $1 ORDER BY line_no"#,
        d.order_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let events = sqlx::query!(
        r#"SELECT e.from_status, e.to_status, e.kind, u.display_name AS actor_name, e.note,
                  e.changes, e.occurred_at
           FROM delivery_events e JOIN users u ON u.id = e.actor_id
           WHERE e.delivery_id = $1
           ORDER BY e.occurred_at, e.id"#,
        id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|e| EventOut {
        changes: e.changes.filter(|_| e.kind == "edit"),
        from: e.from_status,
        to: e.to_status,
        kind: e.kind,
        actor_name: e.actor_name,
        note: e.note,
        at: e.occurred_at,
    })
    .collect();
    let attempts = sqlx::query!(
        r#"SELECT u.display_name AS driver_name, a.reason, a.note, a.trip_pay, a.occurred_at
           FROM delivery_attempts a JOIN users u ON u.id = a.driver_id
           WHERE a.delivery_id = $1
           ORDER BY a.occurred_at, a.id"#,
        id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|a| AttemptOut {
        driver_name: a.driver_name,
        reason: a.reason,
        note: a.note,
        trip_pay: a.trip_pay.map(money),
        at: a.occurred_at,
    })
    .collect();

    let owed = ledger::order_owed(conn, d.order_id).await?;
    let (cash, upi, _) = orders_read::paid_by_method(conn, d.order_id).await?;
    let amount_to_collect = to_collect(&d.collector, owed);
    let driver = match (
        d.current_driver_id,
        d.driver_name,
        d.driver_phone,
        d.driver_active,
    ) {
        (Some(id), Some(name), Some(phone), Some(active)) => Some(DriverOut {
            id,
            name,
            phone,
            active,
        }),
        _ => None,
    };
    let (driver_whatsapp_url, customer_whatsapp_url) = match &driver {
        Some(dr) => {
            let job = Job {
                id: d.id,
                order_number: &d.number,
                recipient_name: &d.recipient_name,
                recipient_phone: &d.recipient_phone,
                address_line: &d.address_line,
                locality: &d.locality,
                landmark: d.landmark.as_deref(),
                instructions: d.instructions.as_deref(),
                amount_to_collect,
            };
            let shop = orders_read::shop_name(conn).await?;
            (
                Some(wa_link(
                    &dr.phone,
                    &messages::driver_text(&job, &cfg.public_url),
                )),
                Some(wa_link(
                    &d.recipient_phone,
                    &messages::customer_text(&shop, &d.number, &dr.name, &dr.phone, owed),
                )),
            )
        }
        None => (None, None),
    };
    Ok(DeliveryDetail {
        maps_url: messages::maps_url(&d.address_line, &d.locality),
        id: d.id,
        order_id: d.order_id,
        status: d.status,
        recipient_name: d.recipient_name,
        recipient_phone: d.recipient_phone,
        address_line: d.address_line,
        locality: d.locality,
        landmark: d.landmark,
        instructions: d.instructions,
        fee: money(d.fee),
        driver_pay: d.driver_pay.map(money),
        collector: d.collector,
        pay_method: d.pay_method,
        received_by: d.received_by,
        created_at: d.created_at,
        assigned_at: d.assigned_at,
        picked_up_at: d.picked_up_at,
        delivered_at: d.delivered_at,
        closed_at: d.closed_at,
        updated_at: d.updated_at,
        order: OrderMoney {
            id: d.order_id,
            number: d.number,
            total: money(d.total),
            paid: money(cash + upi),
            owed: money(owed),
        },
        items,
        driver,
        events,
        attempts,
        amount_to_collect: money(amount_to_collect),
        driver_whatsapp_url,
        customer_whatsapp_url,
    })
}

pub async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<DeliveryList>> {
    user.require("deliveries.read")?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(list(&mut conn, p.status.unwrap_or_default()).await?))
}

pub async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<DeliveryDetail>> {
    user.require("deliveries.read")?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(delivery_detail(&mut conn, id, &s.config).await?))
}
