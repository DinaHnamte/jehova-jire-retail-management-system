//! Later payments applied oldest sale first (Phase 5a Task 8).
mod common;
use axum::{body::Body, http::Request};
use common::{Session, TestApp, TestResponse, app, seed_product, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Shop {
    a: TestApp,
    cashier: Session,
    p: String,
}

async fn shop(pool: &PgPool) -> Shop {
    let a = app(pool.clone());
    let boss_id = seed_user(pool, "boss", PW, &["manager"]).await;
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let boss = a.login("boss", PW).await;
    let p = seed_product(pool, boss_id, "Plate").await.to_string();
    let id = a
        .post_json(
            "/api/v1/stock-counts",
            json!({}),
            Some(&boss.cookie),
            Some(&boss.csrf),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = a
        .put_json(
            &format!("/api/v1/stock-counts/{id}/lines"),
            json!({"lines": [{"product_id": p, "counted": 50, "unit_cost": "60"}]}),
            Some(&boss.cookie),
            Some(&boss.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(
        a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), &boss)
            .await
            .status(),
        200
    );
    let cashier = a.login("cashier", PW).await;
    Shop { a, cashier, p }
}

async fn post(sh: &Shop, s: &Session, path: &str, key: &str, body: Value) -> TestResponse {
    sh.a.send(
        Request::post(path)
            .header("content-type", "application/json")
            .header("cookie", &s.cookie)
            .header("x-csrf-token", &s.csrf)
            .header("idempotency-key", key)
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// A sale of `qty` Plates (100 each) on credit; returns the order JSON.
async fn credit_sale(sh: &Shop, customer: Option<&str>, qty: i64, paid: Option<&str>) -> Value {
    let mut body = json!({"lines": [{"product_id": sh.p, "quantity": qty}]});
    match customer {
        None => body["new_customer"] = json!({"phone": "9862012345", "name": "Mary"}),
        Some(id) => body["customer_id"] = json!(id),
    }
    if let Some(p) = paid {
        body["payments"] = json!([{"method": "cash", "amount": p}]);
    }
    let r = post(
        sh,
        &sh.cashier,
        "/api/v1/orders/checkout",
        &format!("k-{}", Uuid::now_v7()),
        body,
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

async fn pay(sh: &Shop, cid: &str, key: &str, method: &str, amount: &str) -> TestResponse {
    post(
        sh,
        &sh.cashier,
        &format!("/api/v1/customers/{cid}/payments"),
        key,
        json!({"method": method, "amount": amount}),
    )
    .await
}

/// Mary with two unpaid 200 sales; returns (customer id, first order, second order).
async fn mary(sh: &Shop) -> (String, Value, Value) {
    let first = credit_sale(sh, None, 2, None).await;
    let cid = first["customer"]["id"].as_str().unwrap().to_string();
    let second = credit_sale(sh, Some(&cid), 2, None).await;
    (cid, first, second)
}

async fn order(sh: &Shop, o: &Value) -> Value {
    sh.a.get_with(
        &format!("/api/v1/orders/{}", o["id"].as_str().unwrap()),
        Some(&sh.cashier.cookie),
    )
    .await
    .json()
}

#[sqlx::test]
async fn payment_applies_oldest_first(pool: PgPool) {
    let sh = shop(&pool).await;
    let (cid, first, second) = mary(&sh).await;
    let r = pay(&sh, &cid, "key-k1-0000", "upi", "300").await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let b = r.json();
    assert_eq!(b["owes"], "100.00");
    assert_eq!(b["applied"].as_array().unwrap().len(), 2);
    assert_eq!(b["applied"][0]["order_id"], first["id"]);
    assert_eq!(b["applied"][0]["number"], first["number"]);
    assert_eq!(b["applied"][0]["amount"], "200.00");
    assert_eq!(b["applied"][1]["order_id"], second["id"]);
    assert_eq!(b["applied"][1]["amount"], "100.00");
    assert_eq!(order(&sh, &first).await["payment_status"], "paid");
    assert_eq!(order(&sh, &second).await["payment_status"], "partly_paid");
    let audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'customer.payment'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 1);
}

#[sqlx::test]
async fn payment_over_owes_is_refused(pool: PgPool) {
    let sh = shop(&pool).await;
    let (cid, _, _) = mary(&sh).await;
    let r = pay(&sh, &cid, "key-k1-0000", "cash", "400.01").await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "OVERPAYMENT");
    assert_eq!(r.json()["error"]["details"]["max"], "400.00");
    // Nothing was written.
    let d =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}"),
            Some(&sh.cashier.cookie),
        )
        .await
        .json();
    assert_eq!(d["owes"], "400.00");
}

#[sqlx::test]
async fn payment_with_nothing_owed_is_refused(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 1, Some("100")).await;
    let cid = o["customer"]["id"].as_str().unwrap();
    let r = pay(&sh, cid, "key-k1-0000", "cash", "10").await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "OVERPAYMENT");
    assert_eq!(r.json()["error"]["details"]["max"], "0.00");
    assert_eq!(r.json()["error"]["message"], "Nothing is owed.");
}

#[sqlx::test]
async fn payment_validates_input_and_customer(pool: PgPool) {
    let sh = shop(&pool).await;
    let (cid, _, _) = mary(&sh).await;
    let r = pay(&sh, &cid, "key-k1-0000", "cheque", "10").await;
    assert_eq!(
        (r.status(), r.json()["error"]["details"]["field"].clone()),
        (422, json!("method"))
    );
    let r = pay(&sh, &cid, "key-k2-0000", "cash", "0").await;
    assert_eq!(
        (r.status(), r.json()["error"]["details"]["field"].clone()),
        (422, json!("amount"))
    );
    let r = pay(
        &sh,
        &Uuid::now_v7().to_string(),
        "key-k3-0000",
        "cash",
        "10",
    )
    .await;
    assert_eq!(r.status(), 404);
}

#[sqlx::test]
async fn payment_retry_is_idempotent(pool: PgPool) {
    let sh = shop(&pool).await;
    let (cid, _, _) = mary(&sh).await;
    let a = pay(&sh, &cid, "key-same-0000", "cash", "150").await;
    assert_eq!(a.status(), 201);
    let b = pay(&sh, &cid, "key-same-0000", "cash", "150").await;
    assert_eq!(b.status(), 200);
    assert_eq!(a.json()["transaction_id"], b.json()["transaction_id"]);
    let d =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}"),
            Some(&sh.cashier.cookie),
        )
        .await
        .json();
    assert_eq!(d["owes"], "250.00");
}

#[sqlx::test]
async fn concurrent_payments_cannot_overpay(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 2, None).await;
    let cid = o["customer"]["id"].as_str().unwrap().to_string();
    let (a, b) = tokio::join!(
        pay(&sh, &cid, "key-ka-0000", "cash", "150"),
        pay(&sh, &cid, "key-kb-0000", "upi", "150")
    );
    let mut s = vec![a.status(), b.status()];
    s.sort();
    assert_eq!(s, vec![201, 422], "{:?} {:?}", a.json(), b.json());
    let d =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}"),
            Some(&sh.cashier.cookie),
        )
        .await
        .json();
    assert_eq!(d["owes"], "50.00");
}

#[sqlx::test]
async fn payment_needs_payments_record(pool: PgPool) {
    let sh = shop(&pool).await;
    let (cid, _, _) = mary(&sh).await;
    seed_user(&pool, "auditor", PW, &["auditor"]).await;
    let aud = sh.a.login("auditor", PW).await;
    let r = post(
        &sh,
        &aud,
        &format!("/api/v1/customers/{cid}/payments"),
        "key-k1-0000",
        json!({"method": "cash", "amount": "10"}),
    )
    .await;
    assert_eq!(r.status(), 403);
}

#[sqlx::test]
async fn detail_lists_payments_and_reminder(pool: PgPool) {
    let sh = shop(&pool).await;
    let first = credit_sale(&sh, None, 2, Some("50")).await;
    let cid = first["customer"]["id"].as_str().unwrap().to_string();
    let second = credit_sale(&sh, Some(&cid), 2, None).await;
    assert_eq!(
        pay(&sh, &cid, "key-k1-0000", "upi", "300").await.status(),
        201
    );
    let d =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}"),
            Some(&sh.cashier.cookie),
        )
        .await
        .json();
    let pays = d["payments"].as_array().unwrap();
    assert_eq!(pays.len(), 2, "{pays:?}");
    assert_eq!(
        (pays[0]["method"].clone(), pays[0]["amount"].clone()),
        (json!("upi"), json!("300.00"))
    );
    assert_eq!(
        (pays[1]["method"].clone(), pays[1]["amount"].clone()),
        (json!("cash"), json!("50.00"))
    );
    assert_eq!(d["owes"], "50.00");
    let url = d["reminder_url"].as_str().unwrap();
    assert!(
        url.starts_with("https://wa.me/919862012345?text=Hello%20Mary"),
        "{url}"
    );
    let num = second["number"].as_str().unwrap();
    let enc = num.replace('/', "%2F");
    assert!(url.contains(&enc), "{url}");
    assert!(
        !url.contains(&first["number"].as_str().unwrap().replace('/', "%2F")),
        "{url}"
    );

    // Paid up: no reminder.
    assert_eq!(
        pay(&sh, &cid, "key-k2-0000", "cash", "50").await.status(),
        201
    );
    let d =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}"),
            Some(&sh.cashier.cookie),
        )
        .await
        .json();
    assert!(d["reminder_url"].is_null());
}
