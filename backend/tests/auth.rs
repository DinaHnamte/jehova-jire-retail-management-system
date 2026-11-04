mod common;
use common::{app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

fn without_request_id(mut v: Value) -> Value {
    v["error"]["request_id"] = Value::Null;
    v
}

#[sqlx::test]
async fn login_sets_httponly_cookie_and_returns_csrf(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let r = app(pool).login_raw("cashier1", PW).await;
    assert_eq!(r.status(), 200);
    let set = r.header("set-cookie").unwrap();
    assert!(set.starts_with("jj_session="), "{set}");
    for attr in ["HttpOnly", "SameSite=Lax", "Path=/", "Secure"] {
        assert!(set.contains(attr), "missing {attr}: {set}");
    }
    let body = r.json();
    assert!(body["csrf_token"].as_str().unwrap().len() >= 32);
    assert_eq!(body["user"]["username"], "cashier1");
    let perms: Vec<&str> = body["user"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert!(perms.contains(&"orders.create"));
    assert!(!perms.contains(&"users.manage"));
}

#[sqlx::test]
async fn me_requires_session(pool: PgPool) {
    let r = app(pool).get("/api/v1/auth/me").await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.json()["error"]["code"], "UNAUTHENTICATED");
}

#[sqlx::test]
async fn me_with_garbage_cookie_is_401(pool: PgPool) {
    let r = app(pool)
        .get_with("/api/v1/auth/me", Some("jj_session=not-a-real-token"))
        .await;
    assert_eq!(r.status(), 401);
}

#[sqlx::test]
async fn me_with_session_returns_user(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cashier1", PW).await;
    let r = a
        .get_with("/api/v1/auth/me", Some(&format!("other=1; {}", s.cookie)))
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["user"]["username"], "cashier1");
    assert_eq!(r.json()["csrf_token"], s.csrf);
}

#[sqlx::test]
async fn unknown_user_and_wrong_password_look_identical(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let wrong = a.login_raw("cashier1", "nope nope nope").await;
    let unknown = a.login_raw("ghost", "nope nope nope").await;
    assert_eq!(wrong.status(), 401);
    assert_eq!(unknown.status(), 401);
    assert_eq!(wrong.json()["error"]["code"], "INVALID_CREDENTIALS");
    assert_eq!(
        without_request_id(wrong.json()),
        without_request_id(unknown.json())
    );
}

#[sqlx::test]
async fn deactivated_user_cannot_login(pool: PgPool) {
    let id = seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let r = app(pool).login_raw("cashier1", PW).await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.json()["error"]["code"], "INVALID_CREDENTIALS");
}

#[sqlx::test]
async fn deactivated_user_session_is_rejected(pool: PgPool) {
    let id = seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let s = a.login("cashier1", PW).await;
    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&s.cookie))
            .await
            .status(),
        401
    );
}

#[sqlx::test]
async fn expired_session_is_rejected(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let s = a.login("cashier1", PW).await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&s.cookie))
            .await
            .status(),
        401
    );
}

#[sqlx::test]
async fn logout_without_csrf_is_rejected(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cashier1", PW).await;
    let r = a
        .post_json("/api/v1/auth/logout", json!({}), Some(&s.cookie), None)
        .await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json()["error"]["code"], "CSRF_INVALID");
    let r = a
        .post_json(
            "/api/v1/auth/logout",
            json!({}),
            Some(&s.cookie),
            Some("wrong"),
        )
        .await;
    assert_eq!(r.status(), 403);
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&s.cookie))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn post_without_session_is_401(pool: PgPool) {
    let r = app(pool)
        .post_json("/api/v1/auth/logout", json!({}), None, Some("x"))
        .await;
    assert_eq!(r.status(), 401);
}

#[sqlx::test]
async fn logout_with_csrf_ends_session(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cashier1", PW).await;
    let r = a
        .post_json(
            "/api/v1/auth/logout",
            json!({}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 204);
    assert!(r.header("set-cookie").unwrap().contains("Max-Age=0"));
    assert_eq!(
        a.get_with("/api/v1/auth/me", Some(&s.cookie))
            .await
            .status(),
        401
    );
}

#[sqlx::test]
async fn login_is_rate_limited(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    for _ in 0..5 {
        assert_eq!(
            a.login_raw("Cashier1", "wrong password").await.status(),
            401
        );
    }
    let r = a.login_raw("cashier1", PW).await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.json()["error"]["code"], "RATE_LIMITED");
}

#[sqlx::test]
async fn successful_login_resets_failure_count(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool);
    for _ in 0..4 {
        a.login_raw("cashier1", "wrong password").await;
    }
    a.login("cashier1", PW).await;
    for _ in 0..4 {
        a.login_raw("cashier1", "wrong password").await;
    }
    assert_eq!(a.login_raw("cashier1", PW).await.status(), 200);
}

#[sqlx::test]
async fn session_token_is_stored_hashed(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let s = app(pool.clone()).login("cashier1", PW).await;
    let token = s.cookie.trim_start_matches("jj_session=");
    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert_ne!(stored, token.as_bytes());
}

#[sqlx::test]
async fn driver_session_lasts_30_days_staff_12_hours(pool: PgPool) {
    seed_user(&pool, "driver1", PW, &["delivery_driver"]).await;
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool.clone());
    a.login("driver1", PW).await;
    a.login("cashier1", PW).await;
    let secs = |u: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, f64>(
                "SELECT extract(epoch FROM s.expires_at - s.created_at)::float8 FROM sessions s JOIN users u ON u.id = s.user_id WHERE u.username = $1",
            )
            .bind(u)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(secs("driver1").await, 30.0 * 86400.0);
    assert_eq!(secs("cashier1").await, 12.0 * 3600.0);
}

#[sqlx::test]
async fn login_writes_audit_rows_without_password(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = app(pool.clone());
    a.login_raw("cashier1", "wrong password").await;
    a.login("cashier1", PW).await;
    let rows: Vec<(String, Option<Value>)> =
        sqlx::query_as("SELECT action, after FROM audit_logs ORDER BY created_at, action")
            .fetch_all(&pool)
            .await
            .unwrap();
    let actions: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(actions, ["auth.login_failed", "auth.login"]);
    let dump = format!("{rows:?}");
    assert!(!dump.contains("wrong password") && !dump.contains(PW));
}

#[sqlx::test]
async fn malformed_login_body_uses_error_shape(pool: PgPool) {
    let a = app(pool);
    let req = axum::http::Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{not json"))
        .unwrap();
    let r = a.send(req).await;
    assert_eq!(r.status(), 400);
    assert_eq!(r.json()["error"]["code"], "BAD_REQUEST");
}

#[sqlx::test]
async fn overlong_password_is_rejected_before_hashing(pool: PgPool) {
    let r = app(pool).login_raw("x", &"a".repeat(2000)).await;
    assert_eq!(r.status(), 422);
}

#[sqlx::test]
async fn parallel_wrong_passwords_cannot_bypass_rate_limit(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let a = std::sync::Arc::new(app(pool));
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let a = a.clone();
        set.spawn(async move { a.login_raw("cashier1", "wrong password").await.status() });
    }
    let statuses: Vec<u16> = set.join_all().await;
    let verified = statuses.iter().filter(|s| **s == 401).count();
    assert!(
        verified <= 5,
        "{verified} attempts reached password verification: {statuses:?}"
    );
    assert_eq!(
        statuses.iter().filter(|s| **s == 429).count(),
        20 - verified
    );
}

#[sqlx::test]
async fn login_works_with_a_single_connection_pool(pool: PgPool) {
    seed_user(&pool, "cashier1", PW, &["cashier"]).await;
    let one = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let a = app(one);
    assert_eq!(
        a.login_raw("cashier1", "wrong password").await.status(),
        401
    );
    assert_eq!(a.login_raw("cashier1", PW).await.status(), 200);
}
