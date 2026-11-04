pub mod request_id;

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use tower_http::trace::TraceLayer;

use crate::{AppState, MIGRATOR, error::AppError};

pub fn build_app(state: AppState) -> Router {
    let api = Router::new().fallback(|| async { AppError::NotFound });

    Router::new()
        .route("/health/live", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/health/ready", get(ready))
        .nest("/api/v1", api)
        .with_state(state)
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request| {
            let id = req
                .extensions()
                .get::<request_id::RequestId>()
                .map(|r| r.0.to_string())
                .unwrap_or_default();
            tracing::info_span!("request", method = %req.method(), path = %req.uri().path(), request_id = %id)
        }))
        .layer(middleware::from_fn(request_id::middleware))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let check = async {
        let applied: i64 =
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
                .fetch_one(&state.db)
                .await?;
        Ok::<_, sqlx::Error>(applied as usize >= MIGRATOR.iter().count())
    };
    match tokio::time::timeout(Duration::from_secs(2), check).await {
        Ok(Ok(true)) => (
            StatusCode::OK,
            Json(json!({ "database": "ok", "migrations": "ok" })),
        ),
        Ok(Ok(false)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "database": "ok", "migrations": "pending" })),
        ),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "database": "error", "migrations": "unknown" })),
        ),
    }
}
