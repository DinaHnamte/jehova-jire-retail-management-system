//! Orders: checkout at the counter, numbering, pricing rules (spec §3).
pub mod checkout;
pub mod daily;
pub mod domain;
pub mod numbers;
pub mod read;
pub mod whatsapp;

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders/checkout", post(checkout::checkout_route))
        .route("/reports/daily", get(daily::daily_route))
        .route("/orders", get(read::list_route))
        .route("/orders/{id}", get(read::get_route))
        .route("/public/receipts/{token}", get(read::public_receipt_route))
}
