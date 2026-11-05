//! Inventory: stock ledger, suppliers, goods receipts, stock counts (Architecture.md §8, §9).
pub mod domain;
pub mod ledger;
pub mod suppliers;

use uuid::Uuid;

/// The one seeded location; the screens never show it.
pub const SHOP_LOCATION_ID: Uuid = Uuid::from_u128(0x00000000_0000_7000_8002_000000000001);

pub fn router() -> axum::Router<crate::AppState> {
    axum::Router::new().merge(suppliers::router())
}
