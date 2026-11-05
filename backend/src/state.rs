use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{admin::limiter::LoginLimiter, config::Config, shared::rate_limit::RateLimiter};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub login_limiter: Arc<LoginLimiter>,
    /// Public receipt page: 60 requests a minute per client IP.
    pub receipt_limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
            login_limiter: Arc::new(LoginLimiter::default()),
            receipt_limiter: Arc::new(RateLimiter::new(60, Duration::from_secs(60))),
        }
    }

    pub fn with_receipt_limiter(mut self, limiter: RateLimiter) -> Self {
        self.receipt_limiter = Arc::new(limiter);
        self
    }
}
