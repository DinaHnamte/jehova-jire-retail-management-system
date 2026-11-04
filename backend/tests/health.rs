mod common;
use common::app;
use sqlx::PgPool;

#[sqlx::test]
async fn live_is_ok(pool: PgPool) {
    assert_eq!(app(pool).get("/health/live").await.status(), 200);
}

#[sqlx::test]
async fn ready_is_ok_with_db(pool: PgPool) {
    let r = app(pool).get("/health/ready").await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["database"], "ok");
    assert_eq!(r.json()["migrations"], "ok");
}

#[sqlx::test]
async fn ready_is_503_when_db_closed(pool: PgPool) {
    let a = app(pool.clone());
    pool.close().await;
    let r = a.get("/health/ready").await;
    assert_eq!(r.status(), 503);
    assert_eq!(r.json()["database"], "error");
}

#[sqlx::test]
async fn every_response_has_request_id(pool: PgPool) {
    assert!(
        app(pool)
            .get("/health/live")
            .await
            .header("x-request-id")
            .is_some()
    );
}

#[sqlx::test]
async fn valid_incoming_request_id_is_echoed(pool: PgPool) {
    let a = app(pool);
    let id = "0192f0c1-0000-7000-8000-000000000001";
    let req = axum::http::Request::get("/health/live")
        .header("x-request-id", id)
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(
        a.send(req).await.header("x-request-id").as_deref(),
        Some(id)
    );
}
