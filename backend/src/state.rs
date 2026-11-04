use std::sync::Arc;

use sqlx::PgPool;

use crate::{admin::limiter::LoginLimiter, config::Config};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub login_limiter: Arc<LoginLimiter>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
            login_limiter: Arc::new(LoginLimiter::default()),
        }
    }
}
