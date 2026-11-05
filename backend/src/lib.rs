//! Jehovah Jireh retail API. See Architecture.md.
pub mod admin;
pub mod audit;
pub mod catalog;
pub mod config;
pub mod error;
pub mod http;
pub mod inventory;
pub mod shared;
pub mod state;

pub use http::build_app;
pub use state::AppState;

/// Migrations embedded at compile time; also used by `/health/ready`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();
