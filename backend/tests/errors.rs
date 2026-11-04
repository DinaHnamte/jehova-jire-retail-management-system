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

#[sqlx::test]
async fn wrong_method_uses_error_shape(pool: PgPool) {
    let r = app(pool).get("/api/v1/auth/login").await;
    assert_eq!(r.status(), 405);
    assert_eq!(r.json()["error"]["code"], "METHOD_NOT_ALLOWED");
}

#[sqlx::test]
async fn api_root_and_unknown_paths_use_error_shape(pool: PgPool) {
    let a = app(pool);
    for path in ["/api/v1/", "/api/v1", "/nope"] {
        let r = a.get(path).await;
        assert_eq!(r.status(), 404, "{path}");
        assert_eq!(r.json()["error"]["code"], "NOT_FOUND", "{path}");
    }
}
