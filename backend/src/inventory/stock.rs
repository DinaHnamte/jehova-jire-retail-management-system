//! Stock on hand, movement history and adjustments (spec §2 rule 5, §4).
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use std::str::FromStr;
use uuid::Uuid;

use super::{
    SHOP_LOCATION_ID,
    domain::{MovementType, SourceType},
    ledger::{self, MovementInput, Source},
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    catalog::{dto::NamedRef, search::ProductSummary},
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::{
        ids::new_id,
        paging::{Page, Paging},
    },
};

const MAX_QTY: i64 = 1_000_000;

#[derive(Debug, Serialize)]
pub struct StockRow {
    pub product: ProductSummary,
    pub on_hand: i64,
    pub avg_cost: Option<Decimal>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProductStock {
    pub on_hand: i64,
    pub avg_cost: Option<Decimal>,
    pub stock_value: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct MovementOut {
    pub id: Uuid,
    pub movement_type: String,
    pub quantity: i64,
    pub unit_cost: Option<Decimal>,
    pub reason: Option<String>,
    pub source_type: String,
    pub source_id: Uuid,
    pub created_by_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Out,
    UnknownCost,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub q: Option<String>,
    pub filter: Option<Filter>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize)]
pub struct PageParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustRequest {
    #[serde(rename = "type")]
    pub kind: String,
    pub quantity: i64,
    pub reason: String,
}

fn field(name: &str, message: &str) -> AppError {
    AppError::FieldInvalid {
        field: name.into(),
        message: message.into(),
    }
}

/// Active products with their stock; ranked by match when searching, else by code.
pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<StockRow>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let text = crate::catalog::search::query_text(p.q.as_deref());
    let has_q = text.is_some();
    let (raw, norm) = text.unwrap_or_default();
    let filter = match p.filter {
        Some(Filter::Out) => "out",
        Some(Filter::UnknownCost) => "unknown_cost",
        None => "all",
    };
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM products p
           LEFT JOIN catalog_search_hits($1, $2) h ON h.product_id = p.id
           LEFT JOIN stock_balances b ON b.product_id = p.id AND b.location_id = $5
           WHERE p.archived_at IS NULL
             AND (NOT $3 OR h.stage IS NOT NULL)
             AND ($4 = 'all'
                  OR ($4 = 'out' AND COALESCE(b.on_hand, 0) = 0)
                  OR ($4 = 'unknown_cost' AND b.avg_cost IS NULL AND COALESCE(b.on_hand, 0) > 0))"#,
        raw,
        norm,
        has_q,
        filter,
        SHOP_LOCATION_ID
    )
    .fetch_one(db)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT p.id, p.name, pi.normalized_value AS "code?", pi.base_code AS "base_code?",
                  pi.variant AS "variant?", pi.design_code AS "design_code?", p.selling_price, p.mrp,
                  p.unit, p.category_id, c.name AS "category_name?",
                  COALESCE(b.on_hand, 0) AS "on_hand!", b.avg_cost AS "avg_cost?"
           FROM products p
           LEFT JOIN catalog_search_hits($1, $2) h ON h.product_id = p.id
           LEFT JOIN categories c ON c.id = p.category_id
           LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
           LEFT JOIN stock_balances b ON b.product_id = p.id AND b.location_id = $5
           WHERE p.archived_at IS NULL
             AND (NOT $3 OR h.stage IS NOT NULL)
             AND ($4 = 'all'
                  OR ($4 = 'out' AND COALESCE(b.on_hand, 0) = 0)
                  OR ($4 = 'unknown_cost' AND b.avg_cost IS NULL AND COALESCE(b.on_hand, 0) > 0))
           ORDER BY h.stage NULLS LAST, pi.base_code NULLS LAST, pi.variant_num NULLS FIRST,
                    pi.design_code NULLS FIRST, lower(p.name), p.id
           LIMIT $6 OFFSET $7"#,
        raw,
        norm,
        has_q,
        filter,
        SHOP_LOCATION_ID,
        paging.per_page,
        paging.offset()
    )
    .fetch_all(db)
    .await?;
    Ok(Page {
        items: rows
            .into_iter()
            .map(|r| StockRow {
                product: ProductSummary {
                    id: r.id,
                    name: r.name,
                    code: r.code,
                    base_code: r.base_code,
                    variant: r.variant,
                    design_code: r.design_code,
                    selling_price: r.selling_price,
                    mrp: r.mrp,
                    unit: r.unit,
                    category: r
                        .category_id
                        .zip(r.category_name)
                        .map(|(id, name)| NamedRef { id, name }),
                    archived: false,
                },
                on_hand: r.on_hand,
                avg_cost: r.avg_cost,
            })
            .collect(),
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}

/// 404 when the product doesn't exist; otherwise its archived flag.
async fn product_archived(conn: &mut PgConnection, id: Uuid) -> AppResult<bool> {
    sqlx::query_scalar!(
        r#"SELECT archived_at IS NOT NULL AS "archived!" FROM products WHERE id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn product_stock(conn: &mut PgConnection, id: Uuid) -> AppResult<ProductStock> {
    product_archived(&mut *conn, id).await?;
    let row = sqlx::query!(
        "SELECT on_hand, avg_cost FROM stock_balances WHERE product_id = $1 AND location_id = $2",
        id,
        SHOP_LOCATION_ID
    )
    .fetch_optional(conn)
    .await?;
    let (on_hand, avg_cost) = row.map_or((0, None), |r| (r.on_hand, r.avg_cost));
    Ok(ProductStock {
        on_hand,
        avg_cost,
        stock_value: avg_cost.map(|a| (a * Decimal::from(on_hand)).round_dp(2)),
    })
}

pub async fn movements(db: &PgPool, id: Uuid, p: PageParams) -> AppResult<Page<MovementOut>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let mut conn = db.acquire().await?;
    product_archived(&mut conn, id).await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM stock_movements WHERE product_id = $1"#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;
    let items = sqlx::query_as!(
        MovementOut,
        r#"SELECT m.id, m.movement_type, m.quantity, m.unit_cost, m.reason, m.source_type,
                  m.source_id, u.display_name AS created_by_name, m.created_at
           FROM stock_movements m JOIN users u ON u.id = m.created_by
           WHERE m.product_id = $1
           ORDER BY m.created_at DESC, m.id DESC
           LIMIT $2 OFFSET $3"#,
        id,
        paging.per_page,
        paging.offset()
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(Page {
        items,
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}

pub async fn adjust(
    db: &PgPool,
    actor: Uuid,
    id: Uuid,
    req: AdjustRequest,
) -> AppResult<ProductStock> {
    let kind = MovementType::from_str(&req.kind)
        .ok()
        .filter(|t| {
            matches!(
                t,
                MovementType::AdjustmentIn | MovementType::AdjustmentOut | MovementType::Damage
            )
        })
        .ok_or_else(|| field("type", "Choose Add, Remove or Damaged."))?;
    if !(1..=MAX_QTY).contains(&req.quantity) {
        return Err(field("quantity", "Enter a quantity from 1 to 1,000,000."));
    }
    let reason = req.reason.trim().to_string();
    if !(3..=200).contains(&reason.chars().count()) {
        return Err(field("reason", "Say why in 3 to 200 characters."));
    }
    let mut tx = db.begin().await?;
    if product_archived(&mut tx, id).await? {
        return Err(AppError::Validation(
            "This product is archived. Restore it before changing its stock.".into(),
        ));
    }
    let before = product_stock(&mut tx, id).await?;
    let source_id = new_id();
    ledger::apply(
        &mut tx,
        actor,
        Source {
            source_type: SourceType::Adjustment,
            source_id,
        },
        &[MovementInput {
            product_id: id,
            movement_type: kind,
            quantity: req.quantity,
            unit_cost: None,
            reason: Some(reason.clone()),
        }],
    )
    .await?;
    let after = product_stock(&mut tx, id).await?;
    // Keyed by the movement's source id so every adjustment traces to an audit row.
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "stock.adjust",
            entity_type: "stock_adjustment",
            entity_id: Some(source_id),
            before: Some(json!({"product_id": id, "on_hand": before.on_hand})),
            after: Some(json!({
                "product_id": id, "on_hand": after.on_hand, "type": kind.as_str(),
                "quantity": req.quantity, "reason": reason
            })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Cost is internal, as in the catalogue: only people who can edit products see it.
fn sees_cost(user: &CurrentUser) -> bool {
    user.can("products.update")
}

impl ProductStock {
    fn redacted(self, sees_cost: bool) -> Self {
        if sees_cost {
            self
        } else {
            Self {
                avg_cost: None,
                stock_value: None,
                ..self
            }
        }
    }
}

async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<Page<StockRow>>> {
    user.require("inventory.read")?;
    let mut page = list(&s.db, p).await?;
    if !sees_cost(&user) {
        page.items.iter_mut().for_each(|r| r.avg_cost = None);
    }
    Ok(Json(page))
}

async fn product_stock_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ProductStock>> {
    user.require("inventory.read")?;
    Ok(Json(
        product_stock(&mut *s.db.acquire().await?, id)
            .await?
            .redacted(sees_cost(&user)),
    ))
}

async fn movements_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(p): ApiQuery<PageParams>,
) -> AppResult<Json<Page<MovementOut>>> {
    user.require("inventory.read")?;
    let mut page = movements(&s.db, id, p).await?;
    if !sees_cost(&user) {
        page.items.iter_mut().for_each(|m| m.unit_cost = None);
    }
    Ok(Json(page))
}

async fn adjust_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<AdjustRequest>,
) -> AppResult<(StatusCode, Json<ProductStock>)> {
    user.require("inventory.adjust")?;
    Ok((
        StatusCode::CREATED,
        Json(
            adjust(&s.db, user.id, id, req)
                .await?
                .redacted(sees_cost(&user)),
        ),
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stock", get(list_route))
        .route("/products/{id}/stock", get(product_stock_route))
        .route("/products/{id}/movements", get(movements_route))
        .route(
            "/products/{id}/adjustments",
            axum::routing::post(adjust_route),
        )
}
