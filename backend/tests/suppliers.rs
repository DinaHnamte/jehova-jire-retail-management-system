//! Suppliers API (spec §3, §4).
mod common;
use common::{TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn login(pool: &PgPool, role: &str) -> (TestApp, common::Session) {
    seed_user(pool, role, PW, &[role]).await;
    let a = app(pool.clone());
    let s = a.login(role, PW).await;
    (a, s)
}

async fn create(a: &TestApp, s: &common::Session, body: Value) -> common::TestResponse {
    a.post_json("/api/v1/suppliers", body, Some(&s.cookie), Some(&s.csrf))
        .await
}

#[sqlx::test]
async fn stock_operator_creates_supplier(pool: PgPool) {
    let (a, s) = login(&pool, "stock_operator").await;
    let r = create(
        &a,
        &s,
        json!({"name": " Superware Distributors ", "phone": "9862000000"}),
    )
    .await;
    assert_eq!(r.status(), 201);
    let v = r.json();
    assert_eq!(v["name"], "Superware Distributors");
    assert_eq!(v["phone"], "9862000000");
    assert_eq!(v["archived"], false);
}

#[sqlx::test]
async fn duplicate_active_name_conflicts(pool: PgPool) {
    let (a, s) = login(&pool, "manager").await;
    assert_eq!(
        create(&a, &s, json!({"name": "Superware"})).await.status(),
        201
    );
    let r = create(&a, &s, json!({"name": "superware"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.json()["error"]["message"],
        "A supplier called superware already exists."
    );
}

#[sqlx::test]
async fn cashier_cannot_create_supplier(pool: PgPool) {
    let (a, s) = login(&pool, "cashier").await;
    assert_eq!(create(&a, &s, json!({"name": "X"})).await.status(), 403);
}

#[sqlx::test]
async fn cashier_cannot_list_suppliers(pool: PgPool) {
    let (a, s) = login(&pool, "cashier").await;
    assert_eq!(
        a.get_with("/api/v1/suppliers", Some(&s.cookie))
            .await
            .status(),
        403
    );
}

#[sqlx::test]
async fn manager_archives_and_list_hides_archived(pool: PgPool) {
    let (a, s) = login(&pool, "manager").await;
    create(&a, &s, json!({"name": "Alpha"})).await;
    let id = create(&a, &s, json!({"name": "Beta"})).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = a
        .patch_json(
            &format!("/api/v1/suppliers/{id}"),
            json!({"archived": true}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["archived"], true);
    let names = |v: Value| -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap().to_string())
            .collect()
    };
    let r = a.get_with("/api/v1/suppliers", Some(&s.cookie)).await;
    assert_eq!(names(r.json()), ["Alpha"]);
    assert_eq!(r.json()["total"], 1);
    let r = a
        .get_with("/api/v1/suppliers?include_archived=true", Some(&s.cookie))
        .await;
    assert_eq!(names(r.json()), ["Alpha", "Beta"]);
    let r = a.get_with("/api/v1/suppliers?q=alp", Some(&s.cookie)).await;
    assert_eq!(names(r.json()), ["Alpha"]);
}

#[sqlx::test]
async fn supplier_name_too_long_is_422(pool: PgPool) {
    let (a, s) = login(&pool, "manager").await;
    let r = create(&a, &s, json!({"name": "x".repeat(121)})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "name");
    let r = create(&a, &s, json!({"name": "ok", "phone": "1".repeat(21)})).await;
    assert_eq!(r.json()["error"]["details"]["field"], "phone");
}

#[sqlx::test]
async fn supplier_edit_is_audited(pool: PgPool) {
    let (a, s) = login(&pool, "manager").await;
    let id = create(&a, &s, json!({"name": "Alpha"})).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = a
        .patch_json(
            &format!("/api/v1/suppliers/{id}"),
            json!({"phone": "123", "notes": null}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["phone"], "123");
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'supplier.update'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
    let r = a
        .get_with(&format!("/api/v1/suppliers/{id}"), Some(&s.cookie))
        .await;
    assert_eq!(r.json()["name"], "Alpha");
}
