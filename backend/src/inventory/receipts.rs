//! Goods receipts: what arrived from a supplier and what it cost (spec §2 rules 2, 3, 6).
//! A draft adds nothing; confirming writes `purchase_receipt` movements in one transaction.
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{
    check_line_products,
    domain::{MovementType, SourceType},
    ledger::{self, MovementInput, Source},
    line_field, suppliers, today_ist,
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    catalog::dto::{NamedRef, double},
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::{
        ids::new_id,
        money::parse_money,
        paging::{Page, Paging},
    },
};

const MAX_QTY: i64 = 1_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptLineIn {
    pub product_id: Uuid,
    pub quantity: i64,
    pub unit_cost: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinesRequest {
    pub lines: Vec<ReceiptLineIn>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateReceipt {
    pub supplier_id: Uuid,
    pub received_on: Option<NaiveDate>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateReceipt {
    pub supplier_id: Option<Uuid>,
    pub received_on: Option<NaiveDate>,
    #[serde(default, deserialize_with = "double")]
    pub note: Option<Option<String>>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ReceiptLineOut {
    pub product_id: Uuid,
    pub code: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub unit_cost: Decimal,
    pub line_total: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ReceiptOut {
    pub id: Uuid,
    pub supplier: NamedRef,
    pub received_on: NaiveDate,
    pub note: Option<String>,
    pub status: String,
    pub lines: Vec<ReceiptLineOut>,
    pub total: Decimal,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirmed_by_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReceiptSummary {
    pub id: Uuid,
    pub supplier: NamedRef,
    pub received_on: NaiveDate,
    pub status: String,
    pub line_count: i64,
    pub total: Decimal,
}

fn clean_note(note: Option<String>) -> AppResult<Option<String>> {
    let n = note.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if n.as_ref().is_some_and(|s| s.chars().count() > 500) {
        return Err(AppError::FieldInvalid {
            field: "note".into(),
            message: "Keep the note to 500 characters or fewer.".into(),
        });
    }
    Ok(n)
}

async fn require_active_supplier(conn: &mut PgConnection, id: Uuid) -> AppResult<()> {
    if suppliers::active_supplier_name(conn, id).await?.is_none() {
        return Err(AppError::FieldInvalid {
            field: "supplier_id".into(),
            message: "Pick a supplier from the list.".into(),
        });
    }
    Ok(())
}

pub async fn load(conn: &mut PgConnection, id: Uuid) -> AppResult<ReceiptOut> {
    let h = sqlx::query!(
        r#"SELECT r.id, r.supplier_id, s.name AS supplier_name, r.received_on, r.note, r.status,
                  r.confirmed_at, u.display_name AS "confirmed_by_name?"
           FROM goods_receipts r
           JOIN suppliers s ON s.id = r.supplier_id
           LEFT JOIN users u ON u.id = r.confirmed_by
           WHERE r.id = $1"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let lines = sqlx::query_as!(
        ReceiptLineOut,
        r#"SELECT l.product_id,
                  (SELECT raw_value FROM product_identifiers
                    WHERE product_id = l.product_id AND identifier_type = 'product_code') AS code,
                  p.name, l.quantity, l.unit_cost, (l.quantity * l.unit_cost) AS "line_total!"
           FROM goods_receipt_lines l JOIN products p ON p.id = l.product_id
           WHERE l.receipt_id = $1 ORDER BY l.id"#,
        id
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = lines
        .iter()
        .fold(Decimal::new(0, 2), |t, l| t + l.line_total);
    Ok(ReceiptOut {
        id: h.id,
        supplier: NamedRef {
            id: h.supplier_id,
            name: h.supplier_name,
        },
        received_on: h.received_on,
        note: h.note,
        status: h.status,
        lines,
        total,
        confirmed_at: h.confirmed_at,
        confirmed_by_name: h.confirmed_by_name,
    })
}

/// Lock the receipt row; NotFound when missing, NOT_DRAFT unless it is still a draft.
async fn lock_draft(conn: &mut PgConnection, id: Uuid) -> AppResult<()> {
    let status = sqlx::query_scalar!(
        "SELECT status FROM goods_receipts WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)?;
    if status != "draft" {
        return Err(AppError::not_draft("receipt"));
    }
    Ok(())
}

pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<ReceiptSummary>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let status = p.status.filter(|s| s == "draft" || s == "confirmed");
    let mut conn = db.acquire().await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM goods_receipts WHERE $1::text IS NULL OR status = $1"#,
        status.as_deref()
    )
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT r.id, r.supplier_id, s.name AS supplier_name, r.received_on, r.status,
                  (SELECT count(*) FROM goods_receipt_lines WHERE receipt_id = r.id) AS "line_count!",
                  (SELECT COALESCE(sum(quantity * unit_cost), 0) FROM goods_receipt_lines
                    WHERE receipt_id = r.id) AS "total!"
           FROM goods_receipts r JOIN suppliers s ON s.id = r.supplier_id
           WHERE $1::text IS NULL OR r.status = $1
           ORDER BY (r.status = 'draft') DESC, r.received_on DESC, r.id DESC
           LIMIT $2 OFFSET $3"#,
        status.as_deref(),
        paging.per_page,
        paging.offset()
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(Page {
        items: rows
            .into_iter()
            .map(|r| {
                let mut total = r.total;
                total.rescale(2);
                ReceiptSummary {
                    id: r.id,
                    supplier: NamedRef {
                        id: r.supplier_id,
                        name: r.supplier_name,
                    },
                    received_on: r.received_on,
                    status: r.status,
                    line_count: r.line_count,
                    total,
                }
            })
            .collect(),
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}

pub async fn create(db: &PgPool, actor: Uuid, req: CreateReceipt) -> AppResult<ReceiptOut> {
    let note = clean_note(req.note)?;
    let mut tx = db.begin().await?;
    require_active_supplier(&mut tx, req.supplier_id).await?;
    let id = new_id();
    sqlx::query!(
        "INSERT INTO goods_receipts (id, supplier_id, received_on, note, created_by)
         VALUES ($1, $2, $3, $4, $5)",
        id,
        req.supplier_id,
        req.received_on.unwrap_or_else(today_ist),
        note,
        actor
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn update(db: &PgPool, id: Uuid, req: UpdateReceipt) -> AppResult<ReceiptOut> {
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    if let Some(s) = req.supplier_id {
        require_active_supplier(&mut tx, s).await?;
    }
    let (set_note, note) = match req.note {
        Some(n) => (true, clean_note(n)?),
        None => (false, None),
    };
    sqlx::query!(
        "UPDATE goods_receipts SET
           supplier_id = COALESCE($2, supplier_id),
           received_on = COALESCE($3, received_on),
           note = CASE WHEN $4 THEN $5 ELSE note END,
           updated_at = now()
         WHERE id = $1",
        id,
        req.supplier_id,
        req.received_on,
        set_note,
        note
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn delete(db: &PgPool, id: Uuid) -> AppResult<()> {
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    sqlx::query!("DELETE FROM goods_receipts WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn put_lines(db: &PgPool, id: Uuid, lines: Vec<ReceiptLineIn>) -> AppResult<ReceiptOut> {
    let mut costs = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        if !(1..=MAX_QTY).contains(&l.quantity) {
            return Err(line_field(
                i,
                "quantity",
                "Enter a quantity from 1 to 1,000,000.",
            ));
        }
        costs.push(parse_money(&l.unit_cost).map_err(|m| line_field(i, "unit_cost", m))?);
    }
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    check_line_products(&mut tx, &ids, "receipt").await?;
    sqlx::query!("DELETE FROM goods_receipt_lines WHERE receipt_id = $1", id)
        .execute(&mut *tx)
        .await?;
    for (l, cost) in lines.iter().zip(costs) {
        sqlx::query!(
            "INSERT INTO goods_receipt_lines (id, receipt_id, product_id, quantity, unit_cost)
             VALUES ($1, $2, $3, $4, $5)",
            new_id(),
            id,
            l.product_id,
            l.quantity,
            cost
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "UPDATE goods_receipts SET updated_at = now() WHERE id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn confirm(db: &PgPool, actor: Uuid, id: Uuid) -> AppResult<ReceiptOut> {
    let mut tx = db.begin().await?;
    let h = sqlx::query!(
        "SELECT status, supplier_id FROM goods_receipts WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if h.status != "draft" {
        return Err(AppError::already_done(
            "ALREADY_CONFIRMED",
            "This receipt was already confirmed.",
        ));
    }
    let receipt = load(&mut tx, id).await?;
    if receipt.lines.is_empty() {
        return Err(AppError::Validation(
            "Add at least one product before confirming.".into(),
        ));
    }
    if suppliers::active_supplier_name(&mut tx, h.supplier_id)
        .await?
        .is_none()
    {
        return Err(AppError::Validation(format!(
            "{} is archived. Pick another supplier.",
            receipt.supplier.name
        )));
    }
    let ids: Vec<Uuid> = receipt.lines.iter().map(|l| l.product_id).collect();
    check_line_products(&mut tx, &ids, "receipt").await?;
    let moves: Vec<MovementInput> = receipt
        .lines
        .iter()
        .map(|l| MovementInput {
            product_id: l.product_id,
            movement_type: MovementType::PurchaseReceipt,
            quantity: l.quantity,
            unit_cost: Some(l.unit_cost),
            reason: None,
        })
        .collect();
    ledger::apply(
        &mut tx,
        actor,
        Source {
            source_type: SourceType::GoodsReceipt,
            source_id: id,
        },
        &moves,
    )
    .await?;
    sqlx::query!(
        "UPDATE goods_receipts SET status = 'confirmed', confirmed_by = $2, confirmed_at = now(),
                updated_at = now()
         WHERE id = $1",
        id,
        actor
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "goods_receipt.confirm",
            entity_type: "goods_receipt",
            entity_id: Some(id),
            before: None,
            after: Some(json!({
                "total": out.total,
                "supplier_id": out.supplier.id,
                "lines": out.lines.iter().map(|l| json!({
                    "product_id": l.product_id, "quantity": l.quantity, "unit_cost": l.unit_cost
                })).collect::<Vec<_>>(),
            })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(out)
}

async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<Page<ReceiptSummary>>> {
    user.require("purchases.read")?;
    Ok(Json(list(&s.db, p).await?))
}

async fn create_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateReceipt>,
) -> AppResult<(StatusCode, Json<ReceiptOut>)> {
    user.require("purchases.receive")?;
    Ok((
        StatusCode::CREATED,
        Json(create(&s.db, user.id, req).await?),
    ))
}

async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ReceiptOut>> {
    user.require("purchases.read")?;
    Ok(Json(load(&mut *s.db.acquire().await?, id).await?))
}

async fn update_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateReceipt>,
) -> AppResult<Json<ReceiptOut>> {
    user.require("purchases.receive")?;
    Ok(Json(update(&s.db, id, req).await?))
}

async fn delete_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    user.require("purchases.receive")?;
    delete(&s.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lines_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<LinesRequest>,
) -> AppResult<Json<ReceiptOut>> {
    user.require("purchases.receive")?;
    Ok(Json(put_lines(&s.db, id, req.lines).await?))
}

async fn confirm_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ReceiptOut>> {
    user.require("purchases.receive")?;
    Ok(Json(confirm(&s.db, user.id, id).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/goods-receipts", get(list_route).post(create_route))
        .route(
            "/goods-receipts/{id}",
            get(get_route).patch(update_route).delete(delete_route),
        )
        .route("/goods-receipts/{id}/lines", put(lines_route))
        .route("/goods-receipts/{id}/confirm", post(confirm_route))
}
