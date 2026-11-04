//! Gives every request an ID, available to handlers, errors and logs, and
//! returned in the `x-request-id` response header.
use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use uuid::Uuid;

pub const HEADER: &str = "x-request-id";

tokio::task_local! {
    static REQUEST_ID: Uuid;
}

#[derive(Clone, Copy, Debug)]
pub struct RequestId(pub Uuid);

/// The current request's ID, if called while handling a request.
pub fn current() -> Option<Uuid> {
    REQUEST_ID.try_with(|id| *id).ok()
}

pub async fn middleware(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::now_v7);
    req.extensions_mut().insert(RequestId(id));
    let mut res = REQUEST_ID.scope(id, next.run(req)).await;
    res.headers_mut().insert(
        HEADER,
        HeaderValue::from_str(&id.to_string()).expect("uuid is a valid header value"),
    );
    res
}
