//! Driver accounts (Phase 6a §3.5).
mod common;
use common::{TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn owner(pool: &PgPool) -> (TestApp, common::Session) {
    seed_user(pool, "boss", PW, &["admin"]).await;
    let a = app(pool.clone());
    let s = a.login("boss", PW).await;
    (a, s)
}

fn body(username: &str, phone: &str) -> Value {
    json!({"display_name": "Lalbiaka", "username": username, "password": "driver password 1",
           "phone": phone, "default_pay": "50"})
}

async fn add(a: &TestApp, s: &common::Session, b: Value) -> common::TestResponse {
    a.post_json("/api/v1/drivers", b, Some(&s.cookie), Some(&s.csrf))
        .await
}

#[sqlx::test]
async fn owner_adds_driver_who_can_log_in(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let r = add(&a, &s, body("lal", "+91 98620 00000")).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["phone"], "9862000000");
    assert_eq!(v["default_pay"], "50.00");
    assert_eq!(v["is_active"], true);
    assert_eq!(v["open_jobs"], 0);
    let d = a.login("lal", "driver password 1").await;
    let me = a.get_with("/api/v1/auth/me", Some(&d.cookie)).await.json()["user"].clone();
    let mut perms: Vec<String> = me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    perms.sort();
    assert_eq!(
        perms,
        [
            "deliveries.collect_own",
            "deliveries.read_own",
            "deliveries.update_own",
            "settlements.read_own"
        ]
    );
    let list = a.get_with("/api/v1/drivers", Some(&s.cookie)).await.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'driver.create'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 1);
}

#[sqlx::test]
async fn driver_phone_unique(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    assert_eq!(add(&a, &s, body("one", "9862000000")).await.status(), 201);
    let r = add(&a, &s, body("two", "98620 00000")).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "phone");
    assert_eq!(
        r.json()["error"]["message"],
        "Another driver already has this phone number."
    );
    // The failed attempt left no half-made user behind.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE username = 'two'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let r = add(&a, &s, body("one", "9862000001")).await;
    assert_eq!(r.status(), 409);
}

#[sqlx::test]
async fn short_password_is_a_field_error(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let mut b = body("lal", "9862000000");
    b["password"] = json!("short");
    let r = add(&a, &s, b).await;
    assert_eq!(r.status(), 422);
}

async fn make(a: &TestApp, s: &common::Session) -> String {
    add(a, s, body("lal", "9862000000")).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn patch(a: &TestApp, s: &common::Session, id: &str, b: Value) -> common::TestResponse {
    a.patch_json(
        &format!("/api/v1/drivers/{id}"),
        b,
        Some(&s.cookie),
        Some(&s.csrf),
    )
    .await
}

#[sqlx::test]
async fn deactivate_ends_sessions_and_blocks_login(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let id = make(&a, &s).await;
    let d = a.login("lal", "driver password 1").await;
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&d.cookie))
            .await
            .status(),
        200
    );
    let r = patch(&a, &s, &id, json!({"is_active": false})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["is_active"], false);
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&d.cookie))
            .await
            .status(),
        401
    );
    assert_eq!(a.login_raw("lal", "driver password 1").await.status(), 401);
}

#[sqlx::test]
async fn reactivate_allows_login(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let id = make(&a, &s).await;
    patch(&a, &s, &id, json!({"is_active": false})).await;
    let r = patch(
        &a,
        &s,
        &id,
        json!({"is_active": true, "note": " new bike ", "default_pay": "60"}),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["note"], "new bike");
    assert_eq!(r.json()["default_pay"], "60.00");
    assert_eq!(a.login_raw("lal", "driver password 1").await.status(), 200);
    // Inactive drivers sort after active ones.
    add(&a, &s, body("zed", "9862000002")).await;
    patch(&a, &s, &id, json!({"is_active": false})).await;
    let list = a.get_with("/api/v1/drivers", Some(&s.cookie)).await.json();
    assert_eq!(list[0]["username"], "zed");
    assert_eq!(list[1]["username"], "lal");
}

#[sqlx::test]
async fn reset_password_ends_sessions(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let id = make(&a, &s).await;
    let d = a.login("lal", "driver password 1").await;
    let r = a
        .post_json(
            &format!("/api/v1/drivers/{id}/password"),
            json!({"password": "another password 2"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 204);
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&d.cookie))
            .await
            .status(),
        401
    );
    assert_eq!(a.login_raw("lal", "driver password 1").await.status(), 401);
    assert_eq!(a.login_raw("lal", "another password 2").await.status(), 200);
}

#[sqlx::test]
async fn drivers_need_drivers_manage(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let id = make(&a, &s).await;
    seed_user(&pool, "till", PW, &["cashier"]).await;
    let c = a.login("till", PW).await;
    assert_eq!(
        a.get_with("/api/v1/drivers", Some(&c.cookie))
            .await
            .status(),
        403
    );
    assert_eq!(add(&a, &c, body("x", "9862000009")).await.status(), 403);
    assert_eq!(patch(&a, &c, &id, json!({"note": "x"})).await.status(), 403);
    let r = a
        .post_json(
            &format!("/api/v1/drivers/{id}/password"),
            json!({"password": "another password 2"}),
            Some(&c.cookie),
            Some(&c.csrf),
        )
        .await;
    assert_eq!(r.status(), 403);
}

#[sqlx::test]
async fn patch_non_driver_user_is_404(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let cashier = seed_user(&pool, "till", PW, &["cashier"]).await;
    let r = patch(&a, &s, &cashier.to_string(), json!({"is_active": false})).await;
    assert_eq!(r.status(), 404);
    let r = a
        .post_json(
            &format!("/api/v1/drivers/{cashier}/password"),
            json!({"password": "another password 2"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 404);
}

#[sqlx::test]
async fn active_driver_rejects_inactive_and_non_drivers(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let id = make(&a, &s).await;
    let cashier = seed_user(&pool, "till", PW, &["cashier"]).await;
    let mut c = pool.acquire().await.unwrap();
    let uid: uuid::Uuid = id.parse().unwrap();
    let d = jj_retail_api::drivers::active_driver(&mut c, uid)
        .await
        .unwrap();
    assert_eq!(d.phone, "9862000000");
    assert!(
        jj_retail_api::drivers::active_driver(&mut c, cashier)
            .await
            .is_err()
    );
    patch(&a, &s, &id, json!({"is_active": false})).await;
    assert!(
        jj_retail_api::drivers::active_driver(&mut c, uid)
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn driver_username_over_64_is_422(pool: PgPool) {
    let (a, s) = owner(&pool).await;
    let r = add(&a, &s, body(&"u".repeat(65), "9862000000")).await;
    assert_eq!(r.status(), 422, "{:?}", r.json());
    assert_eq!(r.json()["error"]["details"]["field"], "username");
    assert_eq!(
        r.json()["error"]["message"],
        "Keep the username to 64 characters or fewer."
    );
    // 64 characters, padded with spaces, is fine.
    let r = add(
        &a,
        &s,
        body(&format!("  {}  ", "u".repeat(64)), "9862000000"),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
}
