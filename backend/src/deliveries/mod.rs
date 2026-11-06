//! Deliveries: an order sent to a customer's address, with a fee and a driver (Phase 6a).
pub mod actions;
pub mod domain;
pub mod input;
pub mod messages;
pub mod read;

use axum::{
    Router,
    routing::{get, post},
};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{AppState, error::AppResult, shared::ids::new_id};
use domain::Status;
use input::DeliveryIn;

/// Inserts the delivery in `to_send` and its first event. The order row must already exist.
pub async fn create_for_order(
    tx: &mut PgConnection,
    actor: Uuid,
    order_id: Uuid,
    d: &DeliveryIn,
) -> AppResult<Uuid> {
    let id = new_id();
    sqlx::query!(
        "INSERT INTO deliveries
           (id, order_id, status, recipient_name, recipient_phone, address_line, locality,
            landmark, instructions, fee, driver_pay, collector, pay_method)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        id,
        order_id,
        Status::ToSend.as_str(),
        d.recipient_name,
        d.recipient_phone,
        d.address_line,
        d.locality,
        d.landmark,
        d.instructions,
        d.fee,
        d.driver_pay,
        d.collector.as_str(),
        d.pay_method.as_str()
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO delivery_events (id, delivery_id, from_status, to_status, kind, actor_id, changes)
         VALUES ($1, $2, NULL, $3, 'status', $4, $5)",
        new_id(),
        id,
        Status::ToSend.as_str(),
        actor,
        json!({})
    )
    .execute(&mut *tx)
    .await?;
    Ok(id)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/deliveries", get(read::list_route))
        .route(
            "/deliveries/{id}",
            get(read::get_route).patch(actions::edit_route),
        )
        .route("/deliveries/{id}/assign", post(actions::assign_route))
        .route("/deliveries/{id}/status", post(actions::status_route))
        .route("/deliveries/{id}/retry", post(actions::retry_route))
}
