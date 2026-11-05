//! Order detail, list, public receipt and WhatsApp link (Phase 5a Task 7).
mod common;
use axum::{body::Body, http::Request};
use common::{
    Session, TestApp, TestResponse, app, app_with_state, seed_product, seed_user, test_config,
};
use jj_retail_api::{AppState, shared::rate_limit::RateLimiter};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Shop {
    a: TestApp,
    boss: Session,
    cashier: Session,
    p: Vec<String>,
}

async fn opening(a: &TestApp, s: &Session, lines: &[(&str, i64)]) {
    let id = a
        .post_json(
            "/api/v1/stock-counts",
            json!({}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let lines: Vec<Value> = lines
        .iter()
        .map(|(p, n)| json!({"product_id": p, "counted": n, "unit_cost": "60"}))
        .collect();
    let r = a
        .put_json(
            &format!("/api/v1/stock-counts/{id}/lines"),
            json!({"lines": lines}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(
        a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), s)
            .await
            .status(),
        200
    );
}

async fn shop_on(pool: &PgPool, a: TestApp) -> Shop {
    let boss_id = seed_user(pool, "boss", PW, &["manager"]).await;
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let boss = a.login("boss", PW).await;
    let p1 = seed_product(pool, boss_id, "Plate").await.to_string();
    let p2 = seed_product(pool, boss_id, "Bowl").await.to_string();
    opening(&a, &boss, &[(&p1, 50), (&p2, 50)]).await;
    let cashier = a.login("cashier", PW).await;
    Shop {
        a,
        boss,
        cashier,
        p: vec![p1, p2],
    }
}

async fn shop(pool: &PgPool) -> Shop {
    shop_on(pool, app(pool.clone())).await
}

async fn checkout(sh: &Shop, body: Value) -> TestResponse {
    let r =
        sh.a.send(
            Request::post("/api/v1/orders/checkout")
                .header("content-type", "application/json")
                .header("cookie", &sh.cashier.cookie)
                .header("x-csrf-token", &sh.cashier.csrf)
                .header("idempotency-key", format!("key-{}", Uuid::now_v7()))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r
}

/// Two units of Plate at 100, part-paid 80 cash, for Mary.
async fn part_paid(sh: &Shop) -> Value {
    checkout(
        sh,
        json!({"lines": [{"product_id": sh.p[0], "quantity": 2}],
               "new_customer": {"phone": "9862012345", "name": "Mary"},
               "payments": [{"method": "cash", "amount": "80"}]}),
    )
    .await
    .json()
}

async fn get(sh: &Shop, s: &Session, path: &str) -> TestResponse {
    sh.a.get_with(path, Some(&s.cookie)).await
}

#[sqlx::test]
async fn detail_shows_lines_paid_owed_and_links(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = part_paid(&sh).await;
    let r = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let d = r.json();
    assert_eq!(d["payment_status"], "partly_paid");
    assert_eq!(
        (d["total"].as_str(), d["paid"].as_str(), d["owed"].as_str()),
        (Some("200.00"), Some("80.00"), Some("120.00"))
    );
    assert_eq!(d["lines"][0]["product_name"], "Plate");
    assert_eq!(d["lines"][0]["net_amount"], "200.00");
    assert_eq!(d["customer"]["phone"], "9862012345");
    assert_eq!(d["payments"][0]["method"], "cash");
    assert_eq!(d["payments"][0]["amount"], "80.00");
    assert!(
        d["receipt_url"]
            .as_str()
            .unwrap()
            .starts_with("https://shop.test/r/")
    );
    let wa = d["whatsapp_url"].as_str().unwrap();
    assert!(wa.starts_with("https://wa.me/919862012345?text="), "{wa}");
    assert!(wa.contains("Due"), "{wa}");
}

#[sqlx::test]
async fn detail_hides_cost_without_products_update(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = part_paid(&sh).await;
    let path = format!("/api/v1/orders/{}", o["id"].as_str().unwrap());
    let c = get(&sh, &sh.cashier, &path).await.json();
    assert!(c["lines"][0].get("unit_cost").is_none_or(Value::is_null));
    let m = get(&sh, &sh.boss, &path).await.json();
    assert_eq!(m["lines"][0]["unit_cost"], "60.0000");
}

#[sqlx::test]
async fn no_whatsapp_url_without_customer(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = checkout(
        &sh,
        json!({"lines": [{"product_id": sh.p[0], "quantity": 1}],
               "payments": [{"method": "upi", "amount": "100"}]}),
    )
    .await
    .json();
    assert!(o["whatsapp_url"].is_null());
    assert_eq!(o["payment_status"], "paid");
    assert!(o["customer"].is_null());
}

#[sqlx::test]
async fn list_filters_by_paid_status_and_date(pool: PgPool) {
    let sh = shop(&pool).await;
    let partly = part_paid(&sh).await;
    let paid = checkout(
        &sh,
        json!({"lines": [{"product_id": sh.p[1], "quantity": 1}],
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await
    .json();
    let unpaid = checkout(
        &sh,
        json!({"lines": [{"product_id": sh.p[1], "quantity": 1}], "new_customer": {"phone": "9862099999"}}),
    )
    .await
    .json();
    let ids = |v: &Value| -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    };
    let s = |v: &Value| v["id"].as_str().unwrap().to_string();

    let all = get(&sh, &sh.cashier, "/api/v1/orders").await.json();
    assert_eq!(all["total"], 3);
    assert_eq!(
        ids(&all),
        vec![s(&unpaid), s(&paid), s(&partly)],
        "newest first"
    );
    for (f, want) in [
        ("paid", &paid),
        ("partly_paid", &partly),
        ("unpaid", &unpaid),
    ] {
        let r = get(&sh, &sh.cashier, &format!("/api/v1/orders?paid={f}"))
            .await
            .json();
        assert_eq!(ids(&r), vec![s(want)], "{f}");
        assert_eq!(r["total"], 1);
    }
    let today = chrono::Utc::now()
        .with_timezone(&jj_retail_api::inventory::ist())
        .date_naive();
    let tomorrow = today + chrono::Days::new(1);
    let yesterday = today - chrono::Days::new(1);
    let r = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders?from={today}&to={today}"),
    )
    .await
    .json();
    assert_eq!(r["total"], 3);
    let r = get(&sh, &sh.cashier, &format!("/api/v1/orders?from={tomorrow}"))
        .await
        .json();
    assert_eq!(r["total"], 0);
    let r = get(&sh, &sh.cashier, &format!("/api/v1/orders?to={yesterday}"))
        .await
        .json();
    assert_eq!(r["total"], 0);
    let cid = partly["customer"]["id"].as_str().unwrap();
    let r = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders?customer_id={cid}"),
    )
    .await
    .json();
    assert_eq!(ids(&r), vec![s(&partly)]);
    // The customer page shows the same order.
    let c = get(&sh, &sh.cashier, &format!("/api/v1/customers/{cid}"))
        .await
        .json();
    assert_eq!(c["orders"][0]["id"], partly["id"]);
    assert_eq!(c["orders"][0]["owed"], "120.00");
}

#[sqlx::test]
async fn public_receipt_by_token_without_login(pool: PgPool) {
    let sh = shop(&pool).await;
    part_paid(&sh).await;
    let token: String = sqlx::query_scalar("SELECT receipt_token FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    let r = sh.a.get(&format!("/api/v1/public/receipts/{token}")).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let d = r.json();
    assert_eq!(d["shop_name"], "Jehovah Jireh");
    assert_eq!(d["customer_name"], "Mary");
    assert_eq!(d["owed"], "120.00");
    assert_eq!(d["cancelled"], false);
    assert_eq!(d["lines"][0]["name"], "Plate");
    let body = String::from_utf8_lossy(&r.body);
    assert!(
        !body.contains("unit_cost") && !body.contains("avg_cost"),
        "{body}"
    );
}

#[sqlx::test]
async fn public_receipt_unknown_token_404(pool: PgPool) {
    let sh = shop(&pool).await;
    assert_eq!(sh.a.get("/api/v1/public/receipts/nope").await.status(), 404);
}

#[sqlx::test]
async fn public_receipt_rate_limited(pool: PgPool) {
    let state = AppState::new(pool.clone(), test_config())
        .with_receipt_limiter(RateLimiter::new(2, Duration::from_secs(60)));
    let sh = shop_on(&pool, app_with_state(state)).await;
    let path = "/api/v1/public/receipts/nope";
    assert_eq!(sh.a.get(path).await.status(), 404);
    assert_eq!(sh.a.get(path).await.status(), 404);
    let r = sh.a.get(path).await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.json()["error"]["code"], "RATE_LIMITED");
    assert_eq!(
        r.json()["error"]["message"],
        "Too many requests. Wait a minute and try again."
    );
}

#[sqlx::test]
async fn checkout_response_is_order_detail(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = part_paid(&sh).await;
    assert!(o["lines"].is_array() && o["whatsapp_url"].is_string());
    assert_eq!(o["status"], "confirmed");
    assert!(o["number"].as_str().unwrap().starts_with("JJ/"));
}
