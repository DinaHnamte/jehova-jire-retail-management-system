//! Loads the session for every `/api/v1` request and enforces CSRF on state-changing ones.
use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{Method, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;

use crate::{AppState, admin::session, error::AppError};

pub const CSRF_HEADER: &str = "x-csrf-token";
/// Paths (relative to `/api/v1`) that may be POSTed without a session.
const PUBLIC_UNSAFE: &[&str] = &["/auth/login"];
const API_PREFIX: &str = "/api/v1";

pub async fn session_layer(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let user = match session::token_from_headers(req.headers()) {
        None => None,
        Some(token) => {
            let looked_up = async {
                let mut conn = state.db.acquire().await?;
                session::lookup(&mut conn, &token).await
            };
            match looked_up.await {
                Ok(u) => u,
                Err(e) => return e.into_response(),
            }
        }
    };

    let unsafe_method = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let path = req.uri().path();
    // Inside the nested router the prefix may or may not already be stripped.
    let rel = path.strip_prefix(API_PREFIX).unwrap_or(path);
    let public = PUBLIC_UNSAFE.contains(&rel);
    if unsafe_method && !public {
        let Some(user) = &user else {
            return AppError::Unauthenticated.into_response();
        };
        let sent = req
            .headers()
            .get(CSRF_HEADER)
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        if !bool::from(sent.ct_eq(user.csrf_token.as_bytes())) {
            return AppError::CsrfInvalid.into_response();
        }
    }

    if let Some(user) = user {
        req.extensions_mut().insert(user);
    }
    next.run(req).await
}

/// Client IP and user agent for sessions and rate limiting.
pub struct ClientMeta {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl FromRequestParts<AppState> for ClientMeta {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let forwarded = || {
            parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let socket = || {
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip().to_string())
        };
        let ip = if state.config.trust_proxy {
            forwarded().or_else(socket)
        } else {
            socket()
        };
        let user_agent = parts
            .headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(256).collect());
        Ok(Self { ip, user_agent })
    }
}
