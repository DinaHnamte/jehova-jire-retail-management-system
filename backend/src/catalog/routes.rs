//! Catalogue HTTP routes: parse, check permission, call the service.
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
};
use uuid::Uuid;

use super::{
    dto::{
        AddIdentifier, CreateProduct, IdentifierOut, NameRequest, NamedRef, ProductOut,
        UpdateProduct,
    },
    service,
};
use crate::{
    AppState,
    admin::session::CurrentUser,
    error::AppResult,
    http::{json::ApiJson, path::ApiPath, query::ApiQuery},
    shared::paging::Page,
};

use super::search::{self, ListParams, ProductSummary, SearchParams, SearchResponse};

async fn search_products(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<SearchParams>,
) -> AppResult<Json<SearchResponse>> {
    user.require("products.read")?;
    Ok(Json(search::search(&s.db, p).await?))
}

async fn list_products(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiQuery(p): ApiQuery<ListParams>,
) -> AppResult<Json<Page<ProductSummary>>> {
    user.require("products.read")?;
    Ok(Json(search::list(&s.db, p).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/brands", get(list_brands).post(create_brand))
        .route("/categories", get(list_categories).post(create_category))
        .route("/products", get(list_products).post(create_product))
        .route("/products/search", get(search_products))
        .route("/products/{id}", get(get_product).patch(update_product))
        .route("/products/{id}/archive", post(archive_product))
        .route("/products/{id}/restore", post(restore_product))
        .route("/products/{id}/identifiers", post(add_identifier))
        .route(
            "/products/{id}/identifiers/{iid}",
            delete(remove_identifier),
        )
}

/// Cost is internal: only people who can edit products see it.
fn sees_cost(user: &CurrentUser) -> bool {
    user.can("products.update")
}

async fn create_product(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiJson(req): ApiJson<CreateProduct>,
) -> AppResult<(StatusCode, Json<ProductOut>)> {
    user.require("products.create")?;
    let id = service::create_product(&s.db, user.id, req).await?;
    Ok((
        StatusCode::CREATED,
        Json(service::get_product(&s.db, id, sees_cost(&user)).await?),
    ))
}

async fn get_product(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ProductOut>> {
    user.require("products.read")?;
    Ok(Json(
        service::get_product(&s.db, id, sees_cost(&user)).await?,
    ))
}

async fn update_product(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateProduct>,
) -> AppResult<Json<ProductOut>> {
    user.require("products.update")?;
    service::update_product(&s.db, user.id, id, req).await?;
    Ok(Json(
        service::get_product(&s.db, id, sees_cost(&user)).await?,
    ))
}

async fn archive_product(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ProductOut>> {
    user.require("products.archive")?;
    service::set_archived(&s.db, user.id, id, true).await?;
    Ok(Json(
        service::get_product(&s.db, id, sees_cost(&user)).await?,
    ))
}

async fn restore_product(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ProductOut>> {
    user.require("products.archive")?;
    service::set_archived(&s.db, user.id, id, false).await?;
    Ok(Json(
        service::get_product(&s.db, id, sees_cost(&user)).await?,
    ))
}

async fn add_identifier(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<AddIdentifier>,
) -> AppResult<(StatusCode, Json<IdentifierOut>)> {
    user.require("products.update")?;
    Ok((
        StatusCode::CREATED,
        Json(service::add_identifier(&s.db, user.id, id, req).await?),
    ))
}

async fn remove_identifier(
    State(s): State<AppState>,
    user: CurrentUser,
    ApiPath((id, iid)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    user.require("products.update")?;
    service::remove_identifier(&s.db, user.id, id, iid).await?;
    Ok(StatusCode::NO_CONTENT)
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
