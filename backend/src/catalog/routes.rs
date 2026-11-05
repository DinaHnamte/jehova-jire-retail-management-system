//! Catalogue HTTP routes: parse, check permission, call the service.
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};

use super::{
    dto::{NameRequest, NamedRef},
    service,
};
use crate::{AppState, admin::session::CurrentUser, error::AppResult, http::json::ApiJson};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/brands", get(list_brands).post(create_brand))
        .route("/categories", get(list_categories).post(create_category))
}

async fn list_brands(
    State(s): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<NamedRef>>> {
    user.require("products.read")?;
    Ok(Json(service::list_brands(&s.db).await?))
}

async fn create_brand(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<NameRequest>,
) -> AppResult<(StatusCode, Json<NamedRef>)> {
    user.require("products.create")?;
    Ok((
        StatusCode::CREATED,
        Json(service::create_brand(&s.db, &req.name).await?),
    ))
}

async fn list_categories(
    State(s): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<NamedRef>>> {
    user.require("products.read")?;
    Ok(Json(service::list_categories(&s.db).await?))
}

async fn create_category(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<NameRequest>,
) -> AppResult<(StatusCode, Json<NamedRef>)> {
    user.require("products.create")?;
    Ok((
        StatusCode::CREATED,
        Json(service::create_category(&s.db, &req.name).await?),
    ))
}
