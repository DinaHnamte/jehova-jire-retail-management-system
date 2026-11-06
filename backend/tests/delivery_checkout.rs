//! Checkout with a delivery block (Phase 6a Task 2).
mod common;
use common::shop::*;
use serde_json::{Value, json};
use sqlx::PgPool;

fn dblock() -> Value {
    json!({
        "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
        "address_line": "House 12, Chanmari West", "locality": "Chanmari",
        "fee": "50"
    })
}

fn body(sh: &Shop, qty: i64, delivery: Value) -> Value {
    json!({"lines": [{"product_id": sh.p, "quantity": qty}],
           "new_customer": {"phone": "9862012345", "name": "Mary"},
           "delivery": delivery})
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    let sql: &'static str = match table {
        "orders" => "SELECT count(*) FROM orders",
        "deliveries" => "SELECT count(*) FROM deliveries",
        "delivery_events" => "SELECT count(*) FROM delivery_events",
        "customers" => "SELECT count(*) FROM customers",
        _ => panic!("unknown table"),
    };
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn checkout(sh: &Shop, b: Value) -> common::TestResponse {
    post(&sh.a, &sh.c, "/api/v1/orders/checkout", &key(), b).await
}

#[sqlx::test]
async fn delivery_checkout_posts_fee_and_creates_to_send(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(&sh, body(&sh, 2, dblock())).await;
    assert_eq!(o["total"], "250.00");
    assert_eq!(order_owed(&pool, &o).await, d("250"));
    assert_eq!(account_total(&pool, "delivery_fee_income").await, d("-50"));
    assert_eq!(account_total(&pool, "sales_revenue").await, d("-200"));
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
    let (status, collector, pay, fee): (String, String, String, rust_decimal::Decimal) =
        sqlx::query_as("SELECT status, collector, pay_method, fee FROM deliveries")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), collector.as_str(), pay.as_str(), fee),
        ("to_send", "driver", "retain_from_collection", d("50"))
    );
    assert_eq!(count(&pool, "deliveries").await, 1);
    assert_eq!(count(&pool, "delivery_events").await, 1);
    let (f, from, to): (String, Option<String>, String) = sqlx::query_as(
        "SELECT o.fulfilment, e.from_status, e.to_status FROM orders o, delivery_events e",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (f.as_str(), from, to.as_str()),
        ("delivery", None, "to_send")
    );
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn zero_fee_delivery_has_no_fee_entry(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = delivery_sale(&sh, "0", "0").await;
    assert_eq!(o["total"], "100.00");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ledger_entries WHERE account = 'delivery_fee_income'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 0);
    assert_eq!(count(&pool, "deliveries").await, 1);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn delivery_requires_customer_and_address(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = checkout(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 1}], "delivery": dblock()}),
    )
    .await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "customer_id");
    assert_eq!(
        r.json()["error"]["message"],
        "Pick or add the customer for this delivery."
    );

    let mut dl = dblock();
    dl["locality"] = json!("  ");
    let r = checkout(&sh, body(&sh, 1, dl)).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "delivery.locality");
    assert_eq!(count(&pool, "orders").await, 0);
    assert_eq!(count(&pool, "deliveries").await, 0);
    assert_eq!(count(&pool, "customers").await, 0);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
}

#[sqlx::test]
async fn delivery_phone_is_normalised(pool: PgPool) {
    let sh = shop(&pool).await;
    let mut dl = dblock();
    dl["recipient_phone"] = json!("+91 98620-00000");
    sell(&sh, body(&sh, 1, dl)).await;
    let p: String = sqlx::query_scalar("SELECT recipient_phone FROM deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(p, "9862000000");

    let mut dl = dblock();
    dl["recipient_phone"] = json!("12345");
    let r = checkout(&sh, body(&sh, 1, dl)).await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json()["error"]["details"]["field"],
        "delivery.recipient_phone"
    );
}

#[sqlx::test]
async fn retain_with_shop_collector_is_rejected(pool: PgPool) {
    let sh = shop(&pool).await;
    let mut dl = dblock();
    dl["collector"] = json!("shop");
    dl["pay_method"] = json!("retain_from_collection");
    let r = checkout(&sh, body(&sh, 1, dl)).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "delivery.pay_method");

    // The pay method defaults to paid_by_shop when the shop collects.
    let mut dl = dblock();
    dl["collector"] = json!("shop");
    sell(&sh, body(&sh, 1, dl)).await;
    let pm: String = sqlx::query_scalar("SELECT pay_method FROM deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pm, "paid_by_shop");
}

#[sqlx::test]
async fn delivery_overpayment_uses_total_with_fee(pool: PgPool) {
    let sh = shop(&pool).await;
    let mut b = body(&sh, 2, dblock());
    b["payments"] = json!([{"method": "cash", "amount": "251"}]);
    let r = checkout(&sh, b.clone()).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "OVERPAYMENT");
    b["payments"] = json!([{"method": "cash", "amount": "250"}]);
    let o = sell(&sh, b).await;
    assert_eq!(o["owed"], "0.00");
}

#[sqlx::test]
async fn checkout_retry_with_delivery_same_key_once(pool: PgPool) {
    let sh = shop(&pool).await;
    let k = key();
    let b = body(&sh, 1, dblock());
    let r1 = post(&sh.a, &sh.c, "/api/v1/orders/checkout", &k, b.clone()).await;
    assert_eq!(r1.status(), 201);
    let r2 = post(&sh.a, &sh.c, "/api/v1/orders/checkout", &k, b.clone()).await;
    assert_eq!(r2.status(), 200);
    assert_eq!(r1.json()["id"], r2.json()["id"]);
    assert_eq!(count(&pool, "orders").await, 1);
    assert_eq!(count(&pool, "deliveries").await, 1);
    assert_eq!(count(&pool, "delivery_events").await, 1);
    // A different delivery block under the same key is a different request.
    let mut other = b;
    other["delivery"]["fee"] = json!("60");
    let r3 = post(&sh.a, &sh.c, "/api/v1/orders/checkout", &k, other).await;
    assert_eq!(r3.status(), 409, "{:?}", r3.json());
}

#[sqlx::test]
async fn last_address_returns_newest_delivery(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = delivery_sale(&sh, "50", "0").await;
    let cid = o["customer"]["id"].as_str().unwrap().to_string();
    let mut dl = dblock();
    dl["recipient_name"] = json!("Zothansangi");
    dl["locality"] = json!("Dawrpui");
    dl["landmark"] = json!("  ");
    sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 1}],
               "customer_id": cid, "delivery": dl}),
    )
    .await;
    let r =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}/last-address"),
            Some(&sh.c.cookie),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["recipient_name"], "Zothansangi");
    assert_eq!(v["locality"], "Dawrpui");
    assert_eq!(v["recipient_phone"], "9862000000");
    assert!(v["landmark"].is_null());
    assert!(v["instructions"].is_null());
}

#[sqlx::test]
async fn last_address_404_without_delivery(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 1}],
               "new_customer": {"phone": "9862012345"}, "payments": [{"method":"cash","amount":"100"}]}),
    )
    .await;
    let cid = o["customer"]["id"].as_str().unwrap();
    let r =
        sh.a.get_with(
            &format!("/api/v1/customers/{cid}/last-address"),
            Some(&sh.c.cookie),
        )
        .await;
    assert_eq!(r.status(), 404);
}
