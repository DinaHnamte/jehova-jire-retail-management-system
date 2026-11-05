//! Customers API (spec §1).
mod common;
use common::{TestApp, app, seed_user};
use jj_retail_api::customers::find_or_create;
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn login(pool: &PgPool) -> (TestApp, common::Session) {
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let s = a.login("cashier", PW).await;
    (a, s)
}

async fn create(a: &TestApp, s: &common::Session, body: Value) -> common::TestResponse {
    a.post_json("/api/v1/customers", body, Some(&s.cookie), Some(&s.csrf))
        .await
}

#[sqlx::test]
async fn create_normalizes_phone(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let r = create(
        &a,
        &s,
        json!({"phone": "+91 98620-12345", "name": " Mary "}),
    )
    .await;
    assert_eq!(r.status(), 201);
    assert_eq!(r.json()["phone"], "9862012345");
    assert_eq!(r.json()["name"], "Mary");
}

#[sqlx::test]
async fn bad_phone_is_field_error(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let r = create(&a, &s, json!({"phone": "12345"})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "phone");
}

#[sqlx::test]
async fn long_name_is_field_error(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let r = create(
        &a,
        &s,
        json!({"phone": "9862012345", "name": "x".repeat(121)}),
    )
    .await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "name");
}

#[sqlx::test]
async fn duplicate_phone_conflicts(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let first = create(&a, &s, json!({"phone": "9862012345"})).await.json();
    let r = create(&a, &s, json!({"phone": "098620 12345"})).await;
    assert_eq!(r.status(), 409);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "DUPLICATE");
    assert_eq!(e["details"]["customer_id"], first["id"]);
}

#[sqlx::test]
async fn search_by_phone_prefix_and_name(pool: PgPool) {
    let (a, s) = login(&pool).await;
    create(&a, &s, json!({"phone": "9862012345", "name": "Mary"})).await;
    create(&a, &s, json!({"phone": "9436000000", "name": "John"})).await;
    for (q, want) in [("98620", "Mary"), ("0986", "Mary"), ("joh", "John")] {
        let r = a
            .get_with(&format!("/api/v1/customers?q={q}"), Some(&s.cookie))
            .await;
        assert_eq!(r.status(), 200);
        let items = r.json()["items"].as_array().unwrap().clone();
        assert_eq!(items.len(), 1, "q={q}");
        assert_eq!(items[0]["name"], want);
        assert_eq!(items[0]["owes"], "0.00");
    }
}

#[sqlx::test]
async fn detail_shows_zero_owes_for_new_customer(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let c = create(&a, &s, json!({"phone": "9862012345"})).await.json();
    let r = a
        .get_with(
            &format!("/api/v1/customers/{}", c["id"].as_str().unwrap()),
            Some(&s.cookie),
        )
        .await;
    assert_eq!(r.status(), 200);
    let v = r.json();
    assert_eq!(v["owes"], "0.00");
    assert_eq!(v["customer"]["phone"], "9862012345");
    assert_eq!(v["orders"], json!([]));
    assert_eq!(v["payments"], json!([]));
}

#[sqlx::test]
async fn patch_updates_and_rejects_taken_phone(pool: PgPool) {
    let (a, s) = login(&pool).await;
    let m = create(&a, &s, json!({"phone": "9862012345"})).await.json();
    let j = create(&a, &s, json!({"phone": "9436000000"})).await.json();
    let path = format!("/api/v1/customers/{}", m["id"].as_str().unwrap());
    let r = a
        .patch_json(
            &path,
            json!({"name": "Mary", "note": "regular"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["note"], "regular");
    let r = a
        .patch_json(
            &path,
            json!({"phone": "9436000000"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["details"]["customer_id"], j["id"]);
}

#[sqlx::test]
async fn no_permission_is_403(pool: PgPool) {
    seed_user(&pool, "x", PW, &[]).await;
    let a = app(pool.clone());
    let s = a.login("x", PW).await;
    assert_eq!(
        a.get_with("/api/v1/customers", Some(&s.cookie))
            .await
            .status(),
        403
    );
    assert_eq!(
        create(&a, &s, json!({"phone": "9862012345"}))
            .await
            .status(),
        403
    );
}

#[sqlx::test]
async fn find_or_create_reuses_and_fills_name(pool: PgPool) {
    let actor = seed_user(&pool, "cashier", PW, &["cashier"]).await;
    let mut c = pool.acquire().await.unwrap();
    let id = find_or_create(&mut c, actor, "98620 12345", None)
        .await
        .unwrap();
    let again = find_or_create(&mut c, actor, "+919862012345", Some(" Mary "))
        .await
        .unwrap();
    assert_eq!(id, again);
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM customers WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(name.as_deref(), Some("Mary"));
    // An existing name is kept.
    find_or_create(&mut c, actor, "9862012345", Some("Other"))
        .await
        .unwrap();
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM customers WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(name.as_deref(), Some("Mary"));
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'customer.create' AND entity_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *c)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    let bad = find_or_create(&mut c, actor, "123", None).await;
    assert!(
        matches!(bad, Err(jj_retail_api::error::AppError::FieldInvalid { ref field, .. }) if field == "new_customer.phone")
    );
}
