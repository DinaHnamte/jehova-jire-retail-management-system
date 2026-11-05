//! Product catalogue: brands, categories, products, identifiers (Architecture.md §6).
pub mod domain;
pub mod dto;
pub mod repo;
pub mod routes;
pub mod service;

use uuid::Uuid;

/// Seeded brand used when a product doesn't name one.
pub const EKTRA_BRAND_ID: Uuid = Uuid::from_u128(0x00000000_0000_7000_8001_000000000001);

/// True when a database error is a unique-constraint violation.
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}
