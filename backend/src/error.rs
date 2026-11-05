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
    /// A request failed business rules; `details` carries the full report (e.g. an import).
    #[error("{message}")]
    Unprocessable {
        message: String,
        details: serde_json::Value,
    },
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
    /// A business rule with its own error code, e.g. 409 ALREADY_CONFIRMED or 422 INSUFFICIENT_STOCK.
    #[error("{message}")]
    Coded {
        status: StatusCode,
        code: &'static str,
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
            Self::Validation(_) | Self::FieldInvalid { .. } | Self::Unprocessable { .. } => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict(_) | Self::Duplicate { .. } => StatusCode::CONFLICT,
            Self::Coded { status, .. } => *status,
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
            Self::Validation(_) | Self::FieldInvalid { .. } | Self::Unprocessable { .. } => {
                "VALIDATION_FAILED"
            }
            Self::NotFound => "NOT_FOUND",
            Self::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            Self::Conflict(_) => "CONFLICT",
            Self::Duplicate { .. } => "DUPLICATE_CODE",
            Self::Coded { code, .. } => code,
            Self::Internal(_) => "INTERNAL",
        }
    }
}

impl AppError {
    /// 409: the document is already final (confirmed or posted).
    pub fn not_draft(what: &str) -> Self {
        Self::Coded {
            status: StatusCode::CONFLICT,
            code: "NOT_DRAFT",
            message: format!("This {what} is already final and can't be changed."),
            details: json!({}),
        }
    }

    /// 409 with a specific code, for an action that was already done.
    pub fn already_done(code: &'static str, message: &str) -> Self {
        Self::Coded {
            status: StatusCode::CONFLICT,
            code,
            message: message.to_string(),
            details: json!({}),
        }
    }

    /// 422: taking stock out would go below zero.
    pub fn insufficient_stock(on_hand: i64, product: &str) -> Self {
        Self::Coded {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "INSUFFICIENT_STOCK",
            message: format!("Only {on_hand} of {product} in stock. Enter {on_hand} or less."),
            details: json!({ "on_hand": on_hand }),
        }
    }

    /// 400: a money-moving request arrived without a usable Idempotency-Key.
    pub fn idempotency_key_required() -> Self {
        Self::Coded {
            status: StatusCode::BAD_REQUEST,
            code: "IDEMPOTENCY_KEY_REQUIRED",
            message: "Missing Idempotency-Key header. Reload the page and try again.".into(),
            details: json!({}),
        }
    }

    /// 409: the same Idempotency-Key was reused with a different request body.
    pub fn idempotency_conflict() -> Self {
        Self::Coded {
            status: StatusCode::CONFLICT,
            code: "IDEMPOTENCY_CONFLICT",
            message:
                "This request was already sent with different details. Start the action again."
                    .into(),
            details: json!({}),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(err) = &self {
            tracing::error!(error = ?err, "internal error");
        }
        let details = match &self {
            Self::Duplicate { details, .. }
            | Self::Unprocessable { details, .. }
            | Self::Coded { details, .. } => details.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coded_error_renders_status_code_and_details() {
        let e = AppError::insufficient_stock(3, "B 6143-8");
        assert_eq!(e.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(e.code(), "INSUFFICIENT_STOCK");
        let res = e.into_response();
        assert_eq!(res.status(), 422);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["code"], "INSUFFICIENT_STOCK");
        assert_eq!(v["error"]["details"]["on_hand"], 3);
        assert_eq!(
            v["error"]["message"],
            "Only 3 of B 6143-8 in stock. Enter 3 or less."
        );
        assert_eq!(AppError::not_draft("receipt").code(), "NOT_DRAFT");
        assert_eq!(
            AppError::not_draft("receipt").status(),
            StatusCode::CONFLICT
        );
        let d = AppError::already_done("ALREADY_CONFIRMED", "x");
        assert_eq!(
            (d.status(), d.code()),
            (StatusCode::CONFLICT, "ALREADY_CONFIRMED")
        );
    }
}
