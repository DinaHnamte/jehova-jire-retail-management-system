//! Driver accounts: users with the delivery_driver role plus a driver profile (Phase 6a §3.5).
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    admin::{self, session::CurrentUser},
    audit,
    catalog::{dto::double, is_unique_violation},
    error::{AppError, AppResult},
    http::{json::ApiJson, path::ApiPath},
    shared::{money::parse_money, phone::normalize_phone},
};

#[derive(Debug, Serialize, Clone)]
pub struct DriverOut {
    pub id: Uuid,
    pub display_name: String,
    pub username: String,
    pub phone: String,
    pub default_pay: Decimal,
    pub note: Option<String>,
    pub is_active: bool,
    pub open_jobs: i64,
}

/// What assignment needs to know about a driver.
#[derive(Debug, Clone)]
pub struct DriverRef {
    pub id: Uuid,
    pub display_name: String,
    pub phone: String,
    pub default_pay: Decimal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDriver {
    pub display_name: String,
    pub username: String,
    pub password: String,
    pub phone: String,
    pub default_pay: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateDriver {
    pub display_name: Option<String>,
    pub phone: Option<String>,
    pub default_pay: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub note: Option<Option<String>>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetPassword {
    pub password: String,
}

fn field(field: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: field.to_string(),
        message: message.into(),
    }
}

fn clean_name(v: &str) -> AppResult<String> {
    let n = v.trim();
    if n.is_empty() || n.chars().count() > 120 {
        return Err(field(
            "display_name",
            "Enter the driver's name, up to 120 characters.",
        ));
    }
    Ok(n.to_string())
}

fn clean_phone(v: &str) -> AppResult<String> {
    normalize_phone(v).map_err(|m| field("phone", m))
}

fn clean_pay(v: &str) -> AppResult<Decimal> {
    parse_money(v).map_err(|m| field("default_pay", m))
}

fn clean_note(v: Option<String>) -> AppResult<Option<String>> {
    let v = v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if v.as_ref().is_some_and(|s| s.chars().count() > 500) {
        return Err(field("note", "Keep the note to 500 characters or fewer."));
    }
    Ok(v)
}

fn phone_taken() -> AppError {
    field("phone", "Another driver already has this phone number.")
}

async fn fetch(conn: &mut PgConnection, id: Uuid) -> AppResult<DriverOut> {
    sqlx::query_as!(
        DriverOut,
        r#"SELECT u.id, u.display_name, u.username, p.phone, p.default_pay, p.note, u.is_active,
                  (SELECT count(*) FROM deliveries d WHERE d.current_driver_id = u.id) AS "open_jobs!"
           FROM driver_profiles p JOIN users u ON u.id = p.user_id
           WHERE u.id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// An active driver, or a 422 on `driver_id`.
pub async fn active_driver(conn: &mut PgConnection, id: Uuid) -> AppResult<DriverRef> {
    sqlx::query_as!(
        DriverRef,
        r#"SELECT u.id, u.display_name, p.phone, p.default_pay
           FROM users u JOIN driver_profiles p ON p.user_id = u.id
           WHERE u.id = $1 AND u.is_active"#,
        id
    )
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| field("driver_id", "Pick an active driver."))
}

pub async fn list(db: &PgPool) -> AppResult<Vec<DriverOut>> {
    Ok(sqlx::query_as!(
        DriverOut,
        r#"SELECT u.id, u.display_name, u.username, p.phone, p.default_pay, p.note, u.is_active,
                  (SELECT count(*) FROM deliveries d WHERE d.current_driver_id = u.id) AS "open_jobs!"
           FROM driver_profiles p JOIN users u ON u.id = p.user_id
           ORDER BY u.is_active DESC, lower(u.display_name), u.id"#
    )
    .fetch_all(db)
    .await?)
}

pub async fn create(db: &PgPool, actor: Uuid, req: CreateDriver) -> AppResult<DriverOut> {
    let display_name = clean_name(&req.display_name)?;
    if req.username.trim().is_empty() {
        return Err(field("username", "Enter a username."));
    }
    let phone = clean_phone(&req.phone)?;
    let pay = clean_pay(&req.default_pay)?;
    let note = clean_note(req.note)?;
    admin::check_password_length(&req.password).map_err(|e| match e {
        AppError::Validation(m) => field("password", m),
        e => e,
    })?;
    let hash = admin::hash_password_blocking(&req.password).await?;

    let mut tx = db.begin().await?;
    let id = admin::repo::create_user(
        &mut tx,
        &req.username,
        &display_name,
        &hash,
        &["delivery_driver"],
    )
    .await?;
    sqlx::query!(
        "INSERT INTO driver_profiles (user_id, phone, default_pay, note) VALUES ($1, $2, $3, $4)",
        id,
        phone,
        pay,
        note
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            phone_taken()
        } else {
            e.into()
        }
    })?;
    let out = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "driver.create",
            entity_type: "driver",
            entity_id: Some(id),
            before: None,
            after: Some(json!(out)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn update(db: &PgPool, actor: Uuid, id: Uuid, req: UpdateDriver) -> AppResult<DriverOut> {
    let mut tx = db.begin().await?;
    let before = fetch(&mut tx, id).await?;
    sqlx::query!("SELECT id FROM users WHERE id = $1 FOR UPDATE", id)
        .fetch_one(&mut *tx)
        .await?;
    let display_name = match &req.display_name {
        Some(n) => clean_name(n)?,
        None => before.display_name.clone(),
    };
    let phone = match &req.phone {
        Some(p) => clean_phone(p)?,
        None => before.phone.clone(),
    };
    let pay = match &req.default_pay {
        Some(p) => clean_pay(p)?,
        None => before.default_pay,
    };
    let note = match req.note {
        Some(v) => clean_note(v)?,
        None => before.note.clone(),
    };
    let is_active = req.is_active.unwrap_or(before.is_active);

    sqlx::query!(
        "UPDATE users SET display_name = $2, is_active = $3 WHERE id = $1",
        id,
        display_name,
        is_active
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE driver_profiles SET phone = $2, default_pay = $3, note = $4, updated_at = now()
         WHERE user_id = $1",
        id,
        phone,
        pay,
        note
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            phone_taken()
        } else {
            e.into()
        }
    })?;
    if before.is_active && !is_active {
        admin::session::delete_all_for_user(&mut tx, id).await?;
    }
    let after = fetch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "driver.update",
            entity_type: "driver",
            entity_id: Some(id),
            before: Some(json!(before)),
            after: Some(json!(after)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

pub async fn reset_password(
    db: &PgPool,
    actor: Uuid,
    id: Uuid,
    req: ResetPassword,
) -> AppResult<()> {
    admin::check_password_length(&req.password).map_err(|e| match e {
        AppError::Validation(m) => field("password", m),
        e => e,
    })?;
    let hash = admin::hash_password_blocking(&req.password).await?;
    let mut tx = db.begin().await?;
    fetch(&mut tx, id).await?;
    sqlx::query!(
        "UPDATE users SET password_hash = $2 WHERE id = $1",
        id,
        hash
    )
    .execute(&mut *tx)
    .await?;
    admin::session::delete_all_for_user(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "driver.password_reset",
            entity_type: "driver",
            entity_id: Some(id),
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<DriverOut>>> {
    user.require("drivers.manage")?;
    Ok(Json(list(&s.db).await?))
}

async fn create_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateDriver>,
) -> AppResult<(StatusCode, Json<DriverOut>)> {
    user.require("drivers.manage")?;
    Ok((
        StatusCode::CREATED,
        Json(create(&s.db, user.id, req).await?),
    ))
}

async fn update_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateDriver>,
) -> AppResult<Json<DriverOut>> {
    user.require("drivers.manage")?;
    Ok(Json(update(&s.db, user.id, id, req).await?))
}

async fn password_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<ResetPassword>,
) -> AppResult<StatusCode> {
    user.require("drivers.manage")?;
    reset_password(&s.db, user.id, id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/drivers", get(list_route).post(create_route))
        .route("/drivers/{id}", axum::routing::patch(update_route))
        .route("/drivers/{id}/password", post(password_route))
}
