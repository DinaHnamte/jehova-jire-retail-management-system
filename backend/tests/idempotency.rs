//! Idempotency keys for money-moving requests (Phase 5a Task 4).
mod common;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use common::seed_user;
use jj_retail_api::shared::idempotency::{Begin, begin, finish, hash_body, key_from_headers};
use serde_json::json;
use sqlx::PgPool;

const KEY: &str = "key-12345678";

#[sqlx::test]
async fn fresh_then_replay(pool: PgPool) {
    let u = seed_user(&pool, "idem1", "pw-long-enough-1", &["cashier"]).await;
    let h = hash_body(&json!({"a": 1}));
    let resp = json!({"order": "x"});

    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        begin(&mut tx, u, "checkout", KEY, &h).await.unwrap(),
        Begin::Fresh
    ));
    finish(&mut tx, u, "checkout", KEY, &resp).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    match begin(&mut tx, u, "checkout", KEY, &h).await.unwrap() {
        Begin::Replay(v) => assert_eq!(v, resp),
        Begin::Fresh => panic!("expected replay"),
    }
}

#[sqlx::test]
async fn different_body_conflicts(pool: PgPool) {
    let u = seed_user(&pool, "idem2", "pw-long-enough-1", &["cashier"]).await;
    let mut tx = pool.begin().await.unwrap();
    begin(&mut tx, u, "checkout", KEY, &hash_body(&json!({"a": 1})))
        .await
        .unwrap();
    finish(&mut tx, u, "checkout", KEY, &json!({}))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let e = begin(&mut tx, u, "checkout", KEY, &hash_body(&json!({"a": 2})))
        .await
        .err()
        .unwrap();
    assert_eq!(e.code(), "IDEMPOTENCY_CONFLICT");
    assert_eq!(e.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn rolled_back_key_is_free_again(pool: PgPool) {
    let u = seed_user(&pool, "idem3", "pw-long-enough-1", &["cashier"]).await;
    let h = hash_body(&json!({"a": 1}));
    let mut tx = pool.begin().await.unwrap();
    begin(&mut tx, u, "checkout", KEY, &h).await.unwrap();
    tx.rollback().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        begin(&mut tx, u, "checkout", KEY, &h).await.unwrap(),
        Begin::Fresh
    ));
}

#[sqlx::test]
async fn keys_are_per_user(pool: PgPool) {
    let a = seed_user(&pool, "idem4a", "pw-long-enough-1", &["cashier"]).await;
    let b = seed_user(&pool, "idem4b", "pw-long-enough-1", &["cashier"]).await;
    let h = hash_body(&json!({"a": 1}));
    let mut tx = pool.begin().await.unwrap();
    begin(&mut tx, a, "checkout", KEY, &h).await.unwrap();
    finish(&mut tx, a, "checkout", KEY, &json!({}))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        begin(&mut tx, b, "checkout", KEY, &h).await.unwrap(),
        Begin::Fresh
    ));
}

#[test]
fn missing_or_short_header_is_400() {
    let e = key_from_headers(&HeaderMap::new()).err().unwrap();
    assert_eq!(e.code(), "IDEMPOTENCY_KEY_REQUIRED");
    assert_eq!(e.status(), StatusCode::BAD_REQUEST);

    let mut h = HeaderMap::new();
    h.insert("idempotency-key", HeaderValue::from_static("abc"));
    let e = key_from_headers(&h).err().unwrap();
    assert_eq!(e.code(), "IDEMPOTENCY_KEY_REQUIRED");

    h.insert(
        "Idempotency-Key",
        HeaderValue::from_static("  key-12345678  "),
    );
    assert_eq!(key_from_headers(&h).unwrap(), "key-12345678");
}
