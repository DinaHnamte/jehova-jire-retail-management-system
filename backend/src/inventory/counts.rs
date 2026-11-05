//! Stock counts (spec §2 rule 4). Posting records the difference between what was counted and
//! what the ledger says: a product's first count is its opening balance, later ones adjust.
use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{
    SHOP_LOCATION_ID, check_line_products,
    domain::{MovementType, SourceType},
    ledger::{self, MovementInput, Source},
    line_field,
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    catalog::dto::double,
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::{
        ids::new_id,
        money::parse_money,
        paging::{Page, Paging},
    },
};

const MAX_QTY: i64 = 1_000_000;
const COUNT_REASON: &str = "Stock count";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountLineIn {
    pub product_id: Uuid,
    pub counted: i64,
    pub unit_cost: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinesRequest {
    pub lines: Vec<CountLineIn>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CreateCount {
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCount {
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
pub struct CountLineOut {
    pub product_id: Uuid,
    pub code: Option<String>,
    pub name: String,
    pub counted: i64,
    pub unit_cost: Option<Decimal>,
    /// Current balance.
    pub on_hand_now: i64,
    /// On hand at the moment of posting; null while a draft.
    pub expected: Option<i64>,
    /// counted − (expected, or on_hand_now for a draft).
    pub difference: i64,
    /// Draft only: stock for this product moved after the count was started.
    pub changed_since_start: bool,
}

#[derive(Debug, Serialize)]
pub struct CountOut {
    pub id: Uuid,
    pub note: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub posted_at: Option<DateTime<Utc>>,
    pub posted_by_name: Option<String>,
    pub lines: Vec<CountLineOut>,
}

#[derive(Debug, Serialize)]
pub struct CountSummary {
    pub id: Uuid,
    pub note: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub posted_at: Option<DateTime<Utc>>,
    pub line_count: i64,
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

pub async fn load(conn: &mut PgConnection, id: Uuid) -> AppResult<CountOut> {
    let h = sqlx::query!(
        r#"SELECT c.id, c.note, c.status, c.created_at, c.posted_at,
                  u.display_name AS "posted_by_name?"
           FROM stock_counts c LEFT JOIN users u ON u.id = c.posted_by
           WHERE c.id = $1"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let rows = sqlx::query!(
        r#"SELECT l.product_id,
                  (SELECT raw_value FROM product_identifiers
                    WHERE product_id = l.product_id AND identifier_type = 'product_code') AS code,
                  p.name, l.counted, l.unit_cost, l.expected,
                  COALESCE(b.on_hand, 0) AS "on_hand_now!",
                  ($2 = 'draft' AND EXISTS (
                      SELECT 1 FROM stock_movements m
                       WHERE m.product_id = l.product_id AND m.created_at > $3)) AS "changed!"
           FROM stock_count_lines l
           JOIN products p ON p.id = l.product_id
           LEFT JOIN stock_balances b ON b.product_id = l.product_id AND b.location_id = $4
           WHERE l.count_id = $1 ORDER BY l.id"#,
        id,
        h.status,
        h.created_at,
        SHOP_LOCATION_ID
    )
    .fetch_all(&mut *conn)
    .await?;
    let lines = rows
        .into_iter()
        .map(|r| CountLineOut {
            difference: r.counted - r.expected.unwrap_or(r.on_hand_now),
            product_id: r.product_id,
            code: r.code,
            name: r.name,
            counted: r.counted,
            unit_cost: r.unit_cost,
            on_hand_now: r.on_hand_now,
            expected: r.expected,
            changed_since_start: r.changed,
        })
        .collect();
    Ok(CountOut {
        id: h.id,
        note: h.note,
        status: h.status,
        created_at: h.created_at,
        posted_at: h.posted_at,
        posted_by_name: h.posted_by_name,
        lines,
    })
}

/// Lock the count row and its status; NotFound when missing.
async fn lock(conn: &mut PgConnection, id: Uuid) -> AppResult<String> {
    sqlx::query_scalar!(
        "SELECT status FROM stock_counts WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

async fn lock_draft(conn: &mut PgConnection, id: Uuid) -> AppResult<()> {
    if lock(conn, id).await? != "draft" {
        return Err(AppError::not_draft("count"));
    }
    Ok(())
}

pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<CountSummary>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let status = p.status.filter(|s| s == "draft" || s == "posted");
    let mut conn = db.acquire().await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM stock_counts WHERE $1::text IS NULL OR status = $1"#,
        status.as_deref()
    )
    .fetch_one(&mut *conn)
    .await?;
    let items = sqlx::query_as!(
        CountSummary,
        r#"SELECT c.id, c.note, c.status, c.created_at, c.posted_at,
                  (SELECT count(*) FROM stock_count_lines WHERE count_id = c.id) AS "line_count!"
           FROM stock_counts c
           WHERE $1::text IS NULL OR c.status = $1
           ORDER BY (c.status = 'draft') DESC, c.created_at DESC, c.id DESC
           LIMIT $2 OFFSET $3"#,
        status.as_deref(),
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

pub async fn create(db: &PgPool, actor: Uuid, req: CreateCount) -> AppResult<CountOut> {
    let note = clean_note(req.note)?;
    let id = new_id();
    let mut tx = db.begin().await?;
    sqlx::query!(
        "INSERT INTO stock_counts (id, note, created_by) VALUES ($1, $2, $3)",
        id,
        note,
        actor
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn update(db: &PgPool, id: Uuid, req: UpdateCount) -> AppResult<CountOut> {
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    if let Some(n) = req.note {
        let note = clean_note(n)?;
        sqlx::query!(
            "UPDATE stock_counts SET note = $2, updated_at = now() WHERE id = $1",
            id,
            note
        )
        .execute(&mut *tx)
        .await?;
    }
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn delete(db: &PgPool, id: Uuid) -> AppResult<()> {
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    sqlx::query!("DELETE FROM stock_counts WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn put_lines(db: &PgPool, id: Uuid, lines: Vec<CountLineIn>) -> AppResult<CountOut> {
    let mut costs = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        if !(0..=MAX_QTY).contains(&l.counted) {
            return Err(line_field(
                i,
                "counted",
                "Enter a count from 0 to 1,000,000.",
            ));
        }
        let cost = match l
            .unit_cost
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(c) => Some(parse_money(c).map_err(|m| line_field(i, "unit_cost", m))?),
            None => None,
        };
        costs.push(cost);
    }
    let mut tx = db.begin().await?;
    lock_draft(&mut tx, id).await?;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    check_line_products(&mut tx, &ids, "count").await?;
    sqlx::query!("DELETE FROM stock_count_lines WHERE count_id = $1", id)
        .execute(&mut *tx)
        .await?;
    for (l, cost) in lines.iter().zip(costs) {
        sqlx::query!(
            "INSERT INTO stock_count_lines (id, count_id, product_id, counted, unit_cost)
             VALUES ($1, $2, $3, $4, $5)",
            new_id(),
            id,
            l.product_id,
            l.counted,
            cost
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "UPDATE stock_counts SET updated_at = now() WHERE id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    let out = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn post_count(db: &PgPool, actor: Uuid, id: Uuid) -> AppResult<CountOut> {
    let mut tx = db.begin().await?;
    if lock(&mut tx, id).await? != "draft" {
        return Err(AppError::already_done(
            "ALREADY_POSTED",
            "This count was already posted.",
        ));
    }
    let lines = sqlx::query!(
        "SELECT product_id, counted, unit_cost FROM stock_count_lines WHERE count_id = $1 ORDER BY id",
        id
    )
    .fetch_all(&mut *tx)
    .await?;
    if lines.is_empty() {
        return Err(AppError::Validation(
            "Add at least one product before posting.".into(),
        ));
    }
    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    check_line_products(&mut tx, &ids, "count").await?;
    // Locked until commit: nothing can move these products between reading and writing.
    let balances = ledger::lock_balances(&mut tx, &ids).await?;
    let with_history: HashSet<Uuid> = sqlx::query_scalar!(
        "SELECT DISTINCT product_id FROM stock_movements WHERE product_id = ANY($1)",
        &ids
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    let mut moves = Vec::new();
    let mut expected: HashMap<Uuid, i64> = HashMap::new();
    for l in &lines {
        let on_hand = balances.get(&l.product_id).map_or(0, |b| b.0);
        expected.insert(l.product_id, on_hand);
        let diff = l.counted - on_hand;
        let mv = |movement_type, quantity, unit_cost, reason: Option<&str>| MovementInput {
            product_id: l.product_id,
            movement_type,
            quantity,
            unit_cost,
            reason: reason.map(str::to_string),
        };
        if !with_history.contains(&l.product_id) {
            if l.counted > 0 {
                moves.push(mv(
                    MovementType::OpeningBalance,
                    l.counted,
                    l.unit_cost,
                    None,
                ));
            }
        } else if diff > 0 {
            moves.push(mv(
                MovementType::AdjustmentIn,
                diff,
                None,
                Some(COUNT_REASON),
            ));
        } else if diff < 0 {
            moves.push(mv(
                MovementType::AdjustmentOut,
                -diff,
                None,
                Some(COUNT_REASON),
            ));
        }
    }
    if !moves.is_empty() {
        ledger::apply(
            &mut tx,
            actor,
            Source {
                source_type: SourceType::StockCount,
                source_id: id,
            },
            &moves,
        )
        .await?;
    }
    for (product_id, exp) in &expected {
        sqlx::query!(
            "UPDATE stock_count_lines SET expected = $3 WHERE count_id = $1 AND product_id = $2",
            id,
            product_id,
            exp
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "UPDATE stock_counts SET status = 'posted', posted_by = $2, posted_at = now(), updated_at = now()
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
            action: "stock_count.post",
            entity_type: "stock_count",
            entity_id: Some(id),
            before: None,
            after: Some(json!({
                "lines": out.lines.iter().map(|l| json!({
                    "product_id": l.product_id, "expected": l.expected, "counted": l.counted
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
) -> AppResult<Json<Page<CountSummary>>> {
    user.require("inventory.read")?;
    Ok(Json(list(&s.db, p).await?))
}

async fn create_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateCount>,
) -> AppResult<(StatusCode, Json<CountOut>)> {
    user.require("inventory.count")?;
    Ok((
        StatusCode::CREATED,
        Json(create(&s.db, user.id, req).await?),
    ))
}

async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<CountOut>> {
    user.require("inventory.read")?;
    Ok(Json(load(&mut *s.db.acquire().await?, id).await?))
}

async fn update_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateCount>,
) -> AppResult<Json<CountOut>> {
    user.require("inventory.count")?;
    Ok(Json(update(&s.db, id, req).await?))
}

async fn delete_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    user.require("inventory.count")?;
    delete(&s.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lines_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<LinesRequest>,
) -> AppResult<Json<CountOut>> {
    user.require("inventory.count")?;
    Ok(Json(put_lines(&s.db, id, req.lines).await?))
}

async fn post_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<CountOut>> {
    user.require("inventory.count")?;
    Ok(Json(post_count(&s.db, user.id, id).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stock-counts", get(list_route).post(create_route))
        .route(
            "/stock-counts/{id}",
            get(get_route).patch(update_route).delete(delete_route),
        )
        .route("/stock-counts/{id}/lines", put(lines_route))
        .route("/stock-counts/{id}/post", post(post_route))
}
