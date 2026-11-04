mod common;
use common::app;
use sqlx::PgPool;

#[sqlx::test]
async fn unknown_api_route_uses_error_shape(pool: PgPool) {
    let r = app(pool).get("/api/v1/nope").await;
    assert_eq!(r.status(), 404);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "NOT_FOUND");
    assert!(e["message"].is_string());
    assert_eq!(
        e["request_id"].as_str(),
        r.header("x-request-id").as_deref()
    );
}
