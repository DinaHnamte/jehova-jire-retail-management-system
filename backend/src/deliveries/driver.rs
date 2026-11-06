//! A driver's own jobs: the list, the detail and the status call (spec §3.4).
//!
//! Drivers see no prices, costs, other orders, customers or the owner's notes. A job that is
//! not theirs, or never was, is a plain 404.
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    actions::change_status,
    input::{StatusRequest, validate_status},
    messages::maps_url,
    read::{money, to_collect},
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    ledger,
};

#[derive(Debug, Serialize)]
pub struct ActiveItem {
    pub id: Uuid,
    pub order_number: String,
    pub status: String,
    pub locality: String,
    pub recipient_name: String,
    /// 0.00 when the shop collects; the driver is not told what the customer owes the shop.
    pub amount_to_collect: String,
    /// Who collects the money: `driver` or `shop`.
    pub collector: String,
}

/// A finished job: no address, phone or items.
#[derive(Debug, Serialize)]
pub struct HistoryItem {
    pub id: Uuid,
    pub order_number: String,
    pub locality: String,
    pub at: DateTime<Utc>,
    /// The order's current total when the driver was the collector, else 0.00 (what was due
    /// at the door is not stored once the job ends).
    pub amount: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct MyJobs {
    pub active: Vec<ActiveItem>,
    pub history: Vec<HistoryItem>,
}

#[derive(Debug, Serialize)]
pub struct JobItem {
    pub name: String,
    pub code: Option<String>,
    pub quantity: i64,
}

#[derive(Debug, Serialize)]
pub struct JobDetail {
    pub id: Uuid,
    pub order_number: String,
    pub status: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub address_line: String,
    pub locality: String,
    pub landmark: Option<String>,
    pub instructions: Option<String>,
    pub items: Vec<JobItem>,
    pub amount_to_collect: String,
    pub collector: String,
    pub pay_method: String,
    pub driver_pay: Option<String>,
    pub maps_url: String,
}

/// The full job while it is theirs, the history fields otherwise.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum MyJob {
    Active(Box<JobDetail>),
    Past(HistoryItem),
}

pub async fn my_jobs(conn: &mut PgConnection, me: Uuid) -> AppResult<MyJobs> {
    let rows = sqlx::query!(
        r#"SELECT d.id, o.number AS order_number, d.status, d.recipient_name, d.locality,
                  d.collector, r.owed AS "owed!"
           FROM deliveries d
           JOIN orders o ON o.id = d.order_id
           CROSS JOIN LATERAL (SELECT COALESCE(SUM(e.amount), 0) AS owed FROM ledger_entries e
                               WHERE e.account = 'receivable' AND e.ref_id = d.order_id) r
           WHERE d.current_driver_id = $1 AND d.status IN ('assigned', 'picked_up')
           ORDER BY d.assigned_at, d.id"#,
        me
    )
    .fetch_all(&mut *conn)
    .await?;
    let active = rows
        .into_iter()
        .map(|r| ActiveItem {
            amount_to_collect: money(to_collect(&r.collector, r.owed)),
            collector: r.collector,
            id: r.id,
            order_number: r.order_number,
            status: r.status,
            locality: r.locality,
            recipient_name: r.recipient_name,
        })
        .collect();
    let history = sqlx::query!(
        r#"SELECT d.id, o.number AS order_number, d.status, d.locality, d.collector,
                  (o.total - o.delivery_fee + d.fee) AS "total!",
                  COALESCE(x.ended_at, d.delivered_at, d.closed_at, d.updated_at) AS "at!"
           FROM deliveries d
           JOIN orders o ON o.id = d.order_id
           CROSS JOIN LATERAL (SELECT max(a.ended_at) AS ended_at FROM delivery_assignments a
                               WHERE a.delivery_id = d.id AND a.driver_id = $1) x
           WHERE EXISTS (SELECT 1 FROM delivery_assignments a
                         WHERE a.delivery_id = d.id AND a.driver_id = $1)
             AND NOT (d.current_driver_id = $1 AND d.status IN ('assigned', 'picked_up'))
           ORDER BY 7 DESC, d.id
           LIMIT 20"#,
        me
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| HistoryItem {
        amount: history_amount(&r.collector, r.total),
        id: r.id,
        order_number: r.order_number,
        locality: r.locality,
        at: r.at,
        status: r.status,
    })
    .collect();
    Ok(MyJobs { active, history })
}

fn history_amount(collector: &str, total: rust_decimal::Decimal) -> String {
    money(if collector == "driver" {
        total
    } else {
        rust_decimal::Decimal::ZERO
    })
}

pub async fn my_job(conn: &mut PgConnection, me: Uuid, id: Uuid) -> AppResult<MyJob> {
    let d = sqlx::query!(
        r#"SELECT d.id, d.order_id, o.number AS order_number, d.status, d.recipient_name,
                  d.recipient_phone, d.address_line, d.locality, d.landmark, d.instructions,
                  d.collector, d.pay_method, d.driver_pay, d.current_driver_id,
                  (o.total - o.delivery_fee + d.fee) AS "total!",
                  COALESCE((SELECT max(a.ended_at) FROM delivery_assignments a
                             WHERE a.delivery_id = d.id AND a.driver_id = $2),
                           d.delivered_at, d.closed_at, d.updated_at) AS "at!",
                  EXISTS (SELECT 1 FROM delivery_assignments a
                           WHERE a.delivery_id = d.id AND a.driver_id = $2) AS "had!"
           FROM deliveries d JOIN orders o ON o.id = d.order_id
           WHERE d.id = $1"#,
        id,
        me
    )
    .fetch_optional(&mut *conn)
    .await?
    .filter(|d| d.had)
    .ok_or(AppError::NotFound)?;
    if d.current_driver_id != Some(me) || !matches!(d.status.as_str(), "assigned" | "picked_up") {
        return Ok(MyJob::Past(HistoryItem {
            amount: history_amount(&d.collector, d.total),
            id: d.id,
            order_number: d.order_number,
            locality: d.locality,
            at: d.at,
            status: d.status,
        }));
    }
    let items = sqlx::query_as!(
        JobItem,
        r#"SELECT product_name AS name, product_code AS code, quantity
           FROM order_lines WHERE order_id = $1 ORDER BY line_no"#,
        d.order_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let owed = ledger::order_owed(conn, d.order_id).await?;
    Ok(MyJob::Active(Box::new(JobDetail {
        amount_to_collect: money(to_collect(&d.collector, owed)),
        maps_url: maps_url(&d.address_line, &d.locality),
        id: d.id,
        order_number: d.order_number,
        status: d.status,
        recipient_name: d.recipient_name,
        recipient_phone: d.recipient_phone,
        address_line: d.address_line,
        locality: d.locality,
        landmark: d.landmark,
        instructions: d.instructions,
        items,
        collector: d.collector,
        pay_method: d.pay_method,
        driver_pay: d.driver_pay.map(money),
    })))
}

pub async fn list_route(State(s): State<AppState>, user: CurrentUser) -> AppResult<Json<MyJobs>> {
    user.require("deliveries.read_own")?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(my_jobs(&mut conn, user.id).await?))
}

pub async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<MyJob>> {
    user.require("deliveries.read_own")?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(my_job(&mut conn, user.id, id).await?))
}

pub async fn status_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<StatusRequest>,
) -> AppResult<Json<MyJob>> {
    user.require("deliveries.update_own")?;
    let change = validate_status(&req)?;
    let mut tx = s.db.begin().await?;
    change_status(&mut tx, user.id, id, change, Some(user.id)).await?;
    tx.commit().await?;
    let mut conn = s.db.acquire().await?;
    Ok(Json(my_job(&mut conn, user.id, id).await?))
}
