pub mod auth;
pub mod json;
pub mod path;
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
use tower_http::{
    catch_panic::CatchPanicLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::{AppState, MIGRATOR, error::AppError};

pub fn build_app(state: AppState) -> Router {
    let api = Router::new()
        .merge(crate::admin::routes::router())
        .merge(crate::catalog::routes::router())
        .fallback(|| async { AppError::NotFound })
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_layer,
        ));

    Router::new()
        .route("/health/live", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/health/ready", get(ready))
        .nest("/api/v1", api)
        .fallback(|| async { AppError::NotFound })
        .method_not_allowed_fallback(|| async { AppError::MethodNotAllowed })
        .with_state(state)
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request| {
            let id = req
                .extensions()
                .get::<request_id::RequestId>()
                .map(|r| r.0.to_string())
                .unwrap_or_default();
            tracing::info_span!("request", method = %req.method(), path = %req.uri().path(), request_id = %id)
        })
        .on_response(DefaultOnResponse::new().level(Level::INFO)))
        .layer(middleware::from_fn(request_id::middleware))
}

/// A handler panicked: log it and answer with the standard 500 body instead of dropping
/// the connection.
pub fn panic_response(panic: Box<dyn std::any::Any + Send>) -> axum::response::Response {
    let msg = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into());
    AppError::Internal(anyhow::anyhow!("handler panicked: {msg}")).into_response()
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

#[cfg(test)]
mod tests {
    #[test]
    fn panic_renders_standard_internal_error() {
        let res = super::panic_response(Box::new("boom"));
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
