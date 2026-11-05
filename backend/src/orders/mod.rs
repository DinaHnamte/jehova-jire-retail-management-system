//! Orders: checkout at the counter, numbering, pricing rules (spec §3).
pub mod checkout;
pub mod domain;
pub mod numbers;

use axum::{Router, routing::post};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/orders/checkout", post(checkout::checkout_route))
}
