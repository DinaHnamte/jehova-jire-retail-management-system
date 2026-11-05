//! Customer queries and writes. Customers are keyed by their normalised 10-digit phone.
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::dto::{
    CreateCustomer, Customer, CustomerDetail, CustomerRow, ListParams, UpdateCustomer,
};
use crate::{
    audit,
    catalog::is_unique_violation,
    error::{AppError, AppResult},
    ledger, orders,
    shared::{
        ids::new_id,
        paging::{Page, Paging},
        phone::normalize_phone,
    },
};

fn field(field: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: field.to_string(),
        message: message.into(),
    }
}

/// Trim; empty becomes None; longer than `max` is a field error on `name`.
fn clean_text(name: &str, v: Option<String>, max: usize, label: &str) -> AppResult<Option<String>> {
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

fn clean_phone(name: &str, raw: &str) -> AppResult<String> {
    normalize_phone(raw).map_err(|m| field(name, m))
}

fn duplicate(id: Uuid) -> AppError {
    AppError::Coded {
        status: axum::http::StatusCode::CONFLICT,
        code: "DUPLICATE",
        message: "A customer with this phone already exists. Open that customer instead.".into(),
        details: json!({ "customer_id": id.to_string() }),
    }
}

async fn id_by_phone(conn: &mut PgConnection, phone: &str) -> AppResult<Option<Uuid>> {
    Ok(
        sqlx::query_scalar!("SELECT id FROM customers WHERE phone = $1", phone)
            .fetch_optional(conn)
            .await?,
    )
}

/// Map a unique violation on phone to the 409 DUPLICATE error naming the owner.
async fn map_phone_clash(db: &PgPool, e: sqlx::Error, phone: &str) -> AppError {
    if is_unique_violation(&e) {
        if let Ok(mut c) = db.acquire().await
            && let Ok(Some(id)) = id_by_phone(&mut c, phone).await
        {
            return duplicate(id);
        }
        return AppError::Conflict(
            "A customer with this phone already exists. Open that customer instead.".into(),
        );
    }
    e.into()
}

pub async fn fetch(conn: &mut PgConnection, id: Uuid) -> AppResult<Customer> {
    sqlx::query_as!(
        Customer,
        "SELECT id, phone, name, note, created_at FROM customers WHERE id = $1",
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

fn money(d: rust_decimal::Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

/// Phone-prefix digits when `q` looks like a phone (digits, `+`, spaces, dashes), else None.
fn phone_prefix(q: &str) -> Option<String> {
    if !q
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-'))
    {
        return None;
    }
    let stripped = q
        .strip_prefix("+91")
        .or_else(|| q.strip_prefix('0'))
        .unwrap_or(q);
    Some(stripped.chars().filter(char::is_ascii_digit).collect())
}

pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<CustomerRow>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let q = p.q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let (phone_like, name_like): (Option<String>, Option<String>) = match q {
        None => (None, None),
        Some(q) => match phone_prefix(&q) {
            Some(d) if !d.is_empty() => (Some(format!("{d}%")), None),
            Some(_) => (None, None),
            None => (
                None,
                Some(format!(
                    "%{}%",
                    q.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )),
            ),
        },
    };
    let mut conn = db.acquire().await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM customers
           WHERE ($1::text IS NULL OR phone LIKE $1) AND ($2::text IS NULL OR name ILIKE $2)"#,
        phone_like.as_deref(),
        name_like.as_deref()
    )
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT c.id, c.phone, c.name, c.note, c.created_at,
                  (SELECT max(o.created_at) FROM orders o WHERE o.customer_id = c.id AND o.status = 'confirmed') AS last_sale_at
           FROM customers c
           WHERE ($1::text IS NULL OR c.phone LIKE $1) AND ($2::text IS NULL OR c.name ILIKE $2)
           ORDER BY (SELECT max(o.created_at) FROM orders o WHERE o.customer_id = c.id AND o.status = 'confirmed') DESC NULLS LAST,
                    lower(c.name), c.id
           LIMIT $3 OFFSET $4"#,
        phone_like.as_deref(),
        name_like.as_deref(),
        paging.per_page,
        paging.offset()
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        let owes = money(ledger::customer_owes(&mut conn, r.id).await?);
        items.push(CustomerRow {
            customer: Customer {
                id: r.id,
                phone: r.phone,
                name: r.name,
                note: r.note,
                created_at: r.created_at,
            },
            owes,
            last_sale_at: r.last_sale_at,
        });
    }
    Ok(Page {
        items,
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}

pub async fn detail(db: &PgPool, id: Uuid) -> AppResult<CustomerDetail> {
    let mut conn = db.acquire().await?;
    let customer = fetch(&mut conn, id).await?;
    let owes = money(ledger::customer_owes(&mut conn, id).await?);
    let orders = orders::read::list(
        &mut conn,
        orders::read::OrderListParams {
            customer_id: Some(id),
            per_page: Some(50),
            ..Default::default()
        },
    )
    .await?
    .items;
    Ok(CustomerDetail {
        customer,
        owes,
        orders,
        payments: Vec::new(),
    })
}

pub async fn create(db: &PgPool, actor: Uuid, req: CreateCustomer) -> AppResult<Customer> {
    let phone = clean_phone("phone", &req.phone)?;
    let name = clean_text("name", req.name, 120, "name")?;
    let note = clean_text("note", req.note, 500, "note")?;
    let mut tx = db.begin().await?;
    if let Some(id) = id_by_phone(&mut tx, &phone).await? {
        return Err(duplicate(id));
    }
    let id = new_id();
    if let Err(e) = sqlx::query!(
        "INSERT INTO customers (id, phone, name, note, created_by) VALUES ($1, $2, $3, $4, $5)",
        id,
        phone,
        name,
        note,
        actor
    )
    .execute(&mut *tx)
    .await
    {
        drop(tx);
        return Err(map_phone_clash(db, e, &phone).await);
    }
    let out = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "customer.create",
            entity_type: "customer",
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
    req: UpdateCustomer,
) -> AppResult<Customer> {
    let mut tx = db.begin().await?;
    let before = fetch(&mut tx, id).await?;
    let phone = match &req.phone {
        Some(p) => clean_phone("phone", p)?,
        None => before.phone.clone(),
    };
    let name = match req.name {
        Some(v) => clean_text("name", v, 120, "name")?,
        None => before.name.clone(),
    };
    let note = match req.note {
        Some(v) => clean_text("note", v, 500, "note")?,
        None => before.note.clone(),
    };
    if phone != before.phone
        && let Some(other) = id_by_phone(&mut tx, &phone).await?
    {
        return Err(duplicate(other));
    }
    if let Err(e) = sqlx::query!(
        "UPDATE customers SET phone = $2, name = $3, note = $4, updated_at = now() WHERE id = $1",
        id,
        phone,
        name,
        note
    )
    .execute(&mut *tx)
    .await
    {
        drop(tx);
        return Err(map_phone_clash(db, e, &phone).await);
    }
    let after = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "customer.update",
            entity_type: "customer",
            entity_id: Some(id),
            before: Some(json!(before)),
            after: Some(json!(after)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// The customer for this phone, created if new. An existing customer keeps its data, and
/// only gets `name` when it has none. Race-safe on the unique phone.
pub async fn find_or_create(
    conn: &mut PgConnection,
    actor: Uuid,
    phone: &str,
    name: Option<&str>,
) -> AppResult<Uuid> {
    let phone = clean_phone("new_customer.phone", phone)?;
    let name = clean_text("new_customer.name", name.map(str::to_string), 120, "name")?;
    let row = sqlx::query!(
        r#"INSERT INTO customers (id, phone, name, created_by) VALUES ($1, $2, $3, $4)
           ON CONFLICT (phone) DO UPDATE SET name = COALESCE(customers.name, EXCLUDED.name)
           RETURNING id, (xmax = 0) AS "inserted!""#,
        new_id(),
        phone,
        name,
        actor
    )
    .fetch_one(&mut *conn)
    .await?;
    if row.inserted {
        let after = fetch(&mut *conn, row.id).await?;
        audit::record(
            &mut *conn,
            audit::Entry {
                actor_id: Some(actor),
                action: "customer.create",
                entity_type: "customer",
                entity_id: Some(row.id),
                before: None,
                after: Some(json!(after)),
            },
        )
        .await?;
    }
    Ok(row.id)
}
