mod common;
use common::{app, seed_user};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

#[sqlx::test]
async fn users_list_requires_login(pool: PgPool) {
    assert_eq!(app(pool).get("/api/v1/users").await.status(), 401);
}

#[sqlx::test]
async fn users_list_requires_users_manage(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cashier1", PW).await;
    let r = a.get_with("/api/v1/users", Some(&s.cookie)).await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json()["error"]["code"], "FORBIDDEN");
}

#[sqlx::test]
async fn admin_can_list_users(pool: PgPool) {
    seed_user(&pool, "boss", PW, &["admin"]).await;
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("boss", PW).await;
    let r = a.get_with("/api/v1/users", Some(&s.cookie)).await;
    assert_eq!(r.status(), 200);
    let users = r.json();
    let names: Vec<&str> = users
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["username"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["boss", "cashier1"]);
    assert_eq!(users[1]["roles"], serde_json::json!(["cashier"]));
    assert_eq!(users[1]["is_active"], true);
    assert!(users[0].get("password_hash").is_none());
}
