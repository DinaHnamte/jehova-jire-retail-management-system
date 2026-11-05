//! Customers: keyed by phone, created at the till or by hand (spec §1).
pub mod dto;
pub mod payments;
pub mod repo;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use uuid::Uuid;

use crate::{
    AppState,
    admin::session::CurrentUser,
    error::AppResult,
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::paging::Page,
};
use dto::{CreateCustomer, Customer, CustomerDetail, CustomerRow, ListParams, UpdateCustomer};

pub use repo::find_or_create;

async fn list_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<Page<CustomerRow>>> {
    user.require("orders.read")?;
    Ok(Json(repo::list(&s.db, p).await?))
}

async fn create_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateCustomer>,
) -> AppResult<(StatusCode, Json<Customer>)> {
    user.require("orders.confirm")?;
    Ok((
        StatusCode::CREATED,
        Json(repo::create(&s.db, user.id, req).await?),
    ))
}

async fn get_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<CustomerDetail>> {
    user.require("orders.read")?;
    Ok(Json(repo::detail(&s.db, id).await?))
}

async fn update_route(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateCustomer>,
) -> AppResult<Json<Customer>> {
    user.require("orders.confirm")?;
    Ok(Json(repo::update(&s.db, user.id, id, req).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/customers", get(list_route).post(create_route))
        .route("/customers/{id}", get(get_route).patch(update_route))
        .route("/customers/{id}/payments", post(payments::payment_route))
}
