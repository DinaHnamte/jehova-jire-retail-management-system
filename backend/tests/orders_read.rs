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

#[sqlx::test]
async fn receipt_adds_order_discount_back_to_lines(pool: PgPool) {
    let sh = shop(&pool).await;
    let boss_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'boss'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let p3 = seed_product(&pool, boss_id, "Cup").await.to_string();
    opening(&sh.a, &sh.boss, &[(&p3, 10)]).await;
    let o = checkout(
        &sh,
        json!({"lines": [{"product_id": sh.p[0], "quantity": 1},
                         {"product_id": sh.p[1], "quantity": 2},
                         {"product_id": p3, "quantity": 3}],
               "order_discount": "50",
               "new_customer": {"phone": "9862012345"},
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await
    .json();
    let token: String = sqlx::query_scalar("SELECT receipt_token FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    let d =
        sh.a.get(&format!("/api/v1/public/receipts/{token}"))
            .await
            .json();
    let dec = |v: &Value| rust_decimal::Decimal::from_str_exact(v.as_str().unwrap()).unwrap();
    let sum: rust_decimal::Decimal = d["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| dec(&l["net_amount"]))
        .sum();
    assert_eq!(sum, dec(&d["subtotal"]));
    assert_eq!(
        dec(&d["total"]),
        dec(&d["subtotal"]) - dec(&d["order_discount"])
    );
    assert_eq!(d["order_discount"], "50.00");
    assert_eq!(d["total"], "550.00");
    // The WhatsApp text carries the same figures.
    let wa = o["whatsapp_url"].as_str().unwrap();
    assert!(
        wa.contains("Discount%20%E2%80%94%20%E2%88%92%E2%82%B950%0A"),
        "{wa}"
    );
}

async fn post_as(sh: &Shop, s: &Session, path: &str, body: Value) -> TestResponse {
    sh.a.send(
        Request::post(path)
            .header("content-type", "application/json")
            .header("cookie", &s.cookie)
            .header("x-csrf-token", &s.csrf)
            .header("idempotency-key", format!("key-{}", Uuid::now_v7()))
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn line_id(pool: &PgPool, order: &Value) -> Uuid {
    sqlx::query_scalar("SELECT id FROM order_lines WHERE order_id = $1 ORDER BY line_no LIMIT 1")
        .bind(Uuid::parse_str(order["id"].as_str().unwrap()).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Gives back `qty` of the first line; `refund` is the cash method when money goes back.
async fn give_back(
    sh: &Shop,
    pool: &PgPool,
    order: &Value,
    qty: i64,
    refund: Option<&str>,
) -> Value {
    let l = line_id(pool, order).await;
    let mut body =
        json!({"lines": [{"order_line_id": l, "quantity": qty, "disposition": "sellable"}]});
    if let Some(m) = refund {
        body["refund_method"] = json!(m);
    }
    let r = post_as(
        sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}/returns", order["id"].as_str().unwrap()),
        body,
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

async fn cancel(sh: &Shop, order: &Value) -> Value {
    let r = post_as(
        sh,
        &sh.boss,
        &format!("/api/v1/orders/{}/cancel", order["id"].as_str().unwrap()),
        json!({}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    r.json()
}

/// Two Plates at 100: cash 150 and UPI 50.
async fn split_paid(sh: &Shop) -> Value {
    checkout(
        sh,
        json!({"lines": [{"product_id": sh.p[0], "quantity": 2}],
               "payments": [{"method": "cash", "amount": "150"}, {"method": "upi", "amount": "50"}]}),
    )
    .await
    .json()
}

#[sqlx::test]
async fn detail_after_partial_return_shows_returned_quantity_and_return_row(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = part_paid(&sh).await;
    give_back(&sh, &pool, &o, 1, None).await;
    let d = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
    )
    .await
    .json();
    let l = &d["lines"][0];
    assert_eq!(l["id"], json!(line_id(&pool, &o).await));
    assert_eq!(l["quantity"], 2);
    assert_eq!(l["returned_quantity"], 1);
    let r = &d["returns"];
    assert_eq!(r.as_array().unwrap().len(), 1);
    assert_eq!(r[0]["amount"], "100.00");
    assert_eq!(r[0]["owed_reduced"], "100.00");
    assert_eq!(r[0]["refunded"], "0.00");
    assert_eq!(r[0]["refund_method"], Value::Null);
    assert_eq!(r[0]["lines"][0]["product_name"], "Plate");
    assert_eq!(r[0]["lines"][0]["quantity"], 1);
    assert_eq!(r[0]["lines"][0]["disposition"], "sellable");
    assert_eq!(r[0]["lines"][0]["amount"], "100.00");
    assert_eq!(d["owed"], "20.00");
    assert_eq!(d["paid"], "80.00");
    assert_eq!(d["payment_status"], "partly_paid");
}

#[sqlx::test]
async fn detail_paid_by_method_nets_refunds(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = split_paid(&sh).await;
    let before = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
    )
    .await
    .json();
    assert_eq!(
        before["paid_by_method"],
        json!({"cash": "150.00", "upi": "50.00"})
    );
    give_back(&sh, &pool, &o, 1, Some("cash")).await;
    let d = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
    )
    .await
    .json();
    assert_eq!(
        d["paid_by_method"],
        json!({"cash": "50.00", "upi": "50.00"})
    );
    assert_eq!(d["paid"], "100.00");
    assert_eq!(d["owed"], "0.00");
    assert_eq!(d["returns"][0]["refunded"], "100.00");
    assert_eq!(d["returns"][0]["refund_method"], "cash");
}

#[sqlx::test]
async fn cancelled_detail_status_and_payment_status(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = split_paid(&sh).await;
    let d = cancel(&sh, &o).await;
    assert_eq!(d["status"], "cancelled");
    assert_eq!(d["payment_status"], "cancelled");
    assert_eq!(d["paid"], "0.00");
    assert_eq!(d["paid_by_method"], json!({"cash": "0.00", "upi": "0.00"}));
    let g = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
    )
    .await
    .json();
    assert_eq!(g["status"], "cancelled");
    assert_eq!(g["payment_status"], "cancelled");
    let list = get(&sh, &sh.cashier, "/api/v1/orders").await.json();
    assert_eq!(list["items"][0]["payment_status"], "cancelled");
    assert_eq!(list["items"][0]["paid"], "0.00");
}

#[sqlx::test]
async fn list_paid_filter_excludes_cancelled(pool: PgPool) {
    let sh = shop(&pool).await;
    let kept = split_paid(&sh).await;
    let gone = split_paid(&sh).await;
    cancel(&sh, &gone).await;
    let all = get(&sh, &sh.cashier, "/api/v1/orders").await.json();
    assert_eq!(all["total"], 2);
    let paid = get(&sh, &sh.cashier, "/api/v1/orders?paid=paid")
        .await
        .json();
    assert_eq!(paid["total"], 1);
    assert_eq!(paid["items"][0]["id"], kept["id"]);
    // Paid is net of refunds in the list too.
    give_back(&sh, &pool, &kept, 1, Some("upi")).await;
    let all = get(&sh, &sh.cashier, "/api/v1/orders").await.json();
    let row = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == kept["id"])
        .unwrap();
    assert_eq!(row["paid"], "100.00");
}

#[sqlx::test]
async fn list_status_cancelled(pool: PgPool) {
    let sh = shop(&pool).await;
    let kept = split_paid(&sh).await;
    let gone = split_paid(&sh).await;
    cancel(&sh, &gone).await;
    let r = get(&sh, &sh.cashier, "/api/v1/orders?status=cancelled")
        .await
        .json();
    assert_eq!(r["total"], 1);
    assert_eq!(r["items"][0]["id"], gone["id"]);
    let r = get(&sh, &sh.cashier, "/api/v1/orders?status=confirmed")
        .await
        .json();
    assert_eq!(r["total"], 1);
    assert_eq!(r["items"][0]["id"], kept["id"]);
}

#[sqlx::test]
async fn public_receipt_lists_returned_items(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = split_paid(&sh).await;
    give_back(&sh, &pool, &o, 1, Some("cash")).await;
    let token = o["receipt_url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let r = sh.a.get(&format!("/api/v1/public/receipts/{token}")).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["paid"], "100.00");
    assert_eq!(v["refunded"], "100.00");
    let ret = v["returned"].as_array().unwrap();
    assert_eq!(ret.len(), 1);
    assert_eq!(ret[0]["name"], "Plate");
    assert_eq!(ret[0]["quantity"], 1);
    assert_eq!(ret[0]["amount"], "100.00");
    assert!(ret[0].get("code").is_some());
    assert!(!v.to_string().contains("cost"), "{v}");
}

#[sqlx::test]
async fn later_customer_payment_counts_only_its_share(pool: PgPool) {
    let sh = shop(&pool).await;
    let mk = |cust: Value| {
        let mut b = json!({"lines": [{"product_id": sh.p[0], "quantity": 2}]});
        b.as_object_mut()
            .unwrap()
            .extend(cust.as_object().unwrap().clone());
        b
    };
    let a = checkout(
        &sh,
        mk(json!({"new_customer": {"phone": "9862012345", "name": "Mary"}})),
    )
    .await
    .json();
    let cid = a["customer"]["id"].as_str().unwrap().to_string();
    let b = checkout(&sh, mk(json!({"customer_id": cid}))).await.json();
    // 300 clears the older sale (200) and puts 100 on the newer one.
    let r = post_as(
        &sh,
        &sh.cashier,
        &format!("/api/v1/customers/{cid}/payments"),
        json!({"method": "upi", "amount": "300"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let d = get(
        &sh,
        &sh.cashier,
        &format!("/api/v1/orders/{}", b["id"].as_str().unwrap()),
    )
    .await
    .json();
    assert_eq!(d["paid"], "100.00");
    assert_eq!(d["owed"], "100.00");
    assert_eq!(
        d["paid_by_method"],
        json!({"cash": "0.00", "upi": "100.00"})
    );
}
