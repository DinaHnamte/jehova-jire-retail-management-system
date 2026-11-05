//! Catalogue API: brands, categories, products and identifiers (Architecture.md §6, §15).
mod common;
use common::{app, seed_user};
use serde_json::json;
use sqlx::PgPool;

const PW: &str = "correct horse battery";

#[sqlx::test]
async fn brands_list_includes_seeded_ektra(pool: PgPool) {
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cash", PW).await;
    let r = a.get_with("/api/v1/brands", Some(&s.cookie)).await;
    assert_eq!(r.status(), 200);
    let names: Vec<String> = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"Ektra by Superware".to_string()),
        "{names:?}"
    );
}

#[sqlx::test]
async fn cashier_cannot_create_category(pool: PgPool) {
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cash", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "Plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json()["error"]["code"], "FORBIDDEN");
}

#[sqlx::test]
async fn stock_operator_creates_category_and_duplicate_conflicts(pool: PgPool) {
    seed_user(&pool, "stock", PW, &["stock_operator"]).await;
    let a = app(pool);
    let s = a.login("stock", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "Plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 201);
    assert_eq!(r.json()["name"], "Plates");
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 409);
    let r = a.get_with("/api/v1/categories", Some(&s.cookie)).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn category_name_must_not_be_blank(pool: PgPool) {
    seed_user(&pool, "stock", PW, &["stock_operator"]).await;
    let a = app(pool);
    let s = a.login("stock", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "  "}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
    let long = "x".repeat(101);
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": long}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
}
