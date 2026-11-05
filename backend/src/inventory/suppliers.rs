//! Suppliers: who goods are received from (spec §1, §3). Bills and payments come in Phase 5.
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    admin::session::CurrentUser,
    audit,
    catalog::{dto::double, is_unique_violation},
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::{
        ids::new_id,
        paging::{Page, Paging},
    },
};

#[derive(Debug, Serialize, Clone)]
pub struct SupplierOut {
    pub id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub archived: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSupplier {
    pub name: String,
    pub phone: Option<String>,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSupplier {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub notes: Option<Option<String>>,
    pub archived: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub q: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn field(field: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: field.to_string(),
        message: message.into(),
    }
}

fn clean_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 120 {
        return Err(field(
            "name",
            "Enter a supplier name of 1 to 120 characters.",
        ));
    }
    Ok(n.to_string())
}

/// Trim; empty becomes None; longer than `max` is a field error.
fn clean_optional(
    name: &str,
    v: Option<String>,
    max: usize,
    label: &str,
) -> AppResult<Option<String>> {
    let v = v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(s) = &v
        && s.chars().count() > max
    {
        return Err(field(
            name,
            format!("Keep the {label} to {max} characters or fewer."),
        ));
    }
    Ok(v)
}

fn conflict(name: &str) -> AppError {
    AppError::Conflict(format!("A supplier called {name} already exists."))
}

async fn fetch(conn: &mut PgConnection, id: Uuid) -> AppResult<SupplierOut> {
    sqlx::query_as!(
        SupplierOut,
        r#"SELECT id, name, phone, notes, archived_at IS NOT NULL AS "archived!"
           FROM suppliers WHERE id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Name of an active supplier, or None when it is missing or archived.
pub(crate) async fn active_supplier_name(
    conn: &mut PgConnection,
    id: Uuid,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar!(
        "SELECT name FROM suppliers WHERE id = $1 AND archived_at IS NULL",
        id
    )
    .fetch_optional(conn)
    .await?)
}

pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<SupplierOut>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let q = p.q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let pattern = q.map(|s| {
        format!(
            "%{}%",
            s.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let mut conn = db.acquire().await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM suppliers
           WHERE ($1 OR archived_at IS NULL) AND ($2::text IS NULL OR name ILIKE $2)"#,
        p.include_archived,
        pattern.as_deref()
    )
    .fetch_one(&mut *conn)
    .await?;
    let items = sqlx::query_as!(
        SupplierOut,
        r#"SELECT id, name, phone, notes, archived_at IS NOT NULL AS "archived!"
           FROM suppliers
           WHERE ($1 OR archived_at IS NULL) AND ($2::text IS NULL OR name ILIKE $2)
           ORDER BY lower(name), id LIMIT $3 OFFSET $4"#,
        p.include_archived,
        pattern.as_deref(),
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

pub async fn create(db: &PgPool, actor: Uuid, req: CreateSupplier) -> AppResult<SupplierOut> {
    let name = clean_name(&req.name)?;
    let phone = clean_optional("phone", req.phone, 20, "phone number")?;
    let notes = clean_optional("notes", req.notes, 500, "notes")?;
    let id = new_id();
    let mut tx = db.begin().await?;
    sqlx::query!(
        "INSERT INTO suppliers (id, name, phone, notes) VALUES ($1, $2, $3, $4)",
        id,
        name,
        phone,
        notes
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            conflict(&name)
        } else {
            e.into()
        }
    })?;
    let out = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "supplier.create",
            entity_type: "supplier",
            entity_id: Some(id),
            before: None,
            after: Some(json!(out)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn update(
    db: &PgPool,
    actor: Uuid,
    id: Uuid,
    req: UpdateSupplier,
) -> AppResult<SupplierOut> {
    let mut tx = db.begin().await?;
    let before = fetch(&mut tx, id).await?;
    let name = match &req.name {
        Some(n) => clean_name(n)?,
        None => before.name.clone(),
    };
    let phone = match req.phone {
        Some(v) => clean_optional("phone", v, 20, "phone number")?,
        None => before.phone.clone(),
    };
    let notes = match req.notes {
        Some(v) => clean_optional("notes", v, 500, "notes")?,
        None => before.notes.clone(),
    };
    let archived = req.archived.unwrap_or(before.archived);
    sqlx::query!(
        "UPDATE suppliers SET name = $2, phone = $3, notes = $4,
                archived_at = CASE WHEN $5 THEN COALESCE(archived_at, now()) ELSE NULL END,
                updated_at = now()
         WHERE id = $1",
        id,
        name,
        phone,
        notes,
        archived
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            conflict(&name)
        } else {
            e.into()
        }
    })?;
    let after = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "supplier.update",
            entity_type: "supplier",
            entity_id: Some(id),
            before: Some(json!(before)),
            after: Some(json!(after)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<Page<SupplierOut>>> {
    user.require("purchases.read")?;
    Ok(Json(list(&s.db, p).await?))
}

async fn create_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateSupplier>,
) -> AppResult<(StatusCode, Json<SupplierOut>)> {
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
) -> AppResult<Json<SupplierOut>> {
    user.require("purchases.read")?;
    Ok(Json(fetch(&mut *s.db.acquire().await?, id).await?))
}

async fn update_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateSupplier>,
) -> AppResult<Json<SupplierOut>> {
    user.require("purchases.receive")?;
    Ok(Json(update(&s.db, user.id, id, req).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/suppliers", get(list_route).post(create_route))
        .route("/suppliers/{id}", get(get_route).patch(update_route))
}
