use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::http::request_id;

/// Every API error. Rendered as `{ "error": { code, message, details, request_id } }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("You need to log in.")]
    Unauthenticated,
    #[error("You don't have permission to do that.")]
    Forbidden,
    #[error("Security token missing or invalid. Reload the page and try again.")]
    CsrfInvalid,
    #[error("Wrong username or password.")]
    InvalidCredentials,
    #[error("Too many attempts. Wait 15 minutes and try again.")]
    RateLimited,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Validation(String),
    /// A business rule failed on one input field; `details.field` names it for the form.
    #[error("{message}")]
    FieldInvalid { field: String, message: String },
    #[error("Not found.")]
    NotFound,
    #[error("That method isn't allowed here.")]
    MethodNotAllowed,
    #[error("{0}")]
    Conflict(String),
    /// A product code (or other identifier) that is already taken; `details` names the owner.
    #[error("{message}")]
    Duplicate {
        message: String,
        details: serde_json::Value,
    },
    #[error("Something went wrong on the server.")]
    Internal(#[source] anyhow::Error),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated | Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::CsrfInvalid => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Validation(_) | Self::FieldInvalid { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict(_) | Self::Duplicate { .. } => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::Forbidden => "FORBIDDEN",
            Self::CsrfInvalid => "CSRF_INVALID",
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::RateLimited => "RATE_LIMITED",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Validation(_) | Self::FieldInvalid { .. } => "VALIDATION_FAILED",
            Self::NotFound => "NOT_FOUND",
            Self::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            Self::Conflict(_) => "CONFLICT",
            Self::Duplicate { .. } => "DUPLICATE_CODE",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(err) = &self {
            tracing::error!(error = ?err, "internal error");
        }
        let details = match &self {
            Self::Duplicate { details, .. } => details.clone(),
            Self::FieldInvalid { field, .. } => json!({ "field": field }),
            _ => json!({}),
        };
        let body = json!({
            "error": {
                "code": self.code(),
                "message": self.to_string(),
                "details": details,
                "request_id": request_id::current().map(|id| id.to_string()),
            }
        });
        (self.status(), Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
