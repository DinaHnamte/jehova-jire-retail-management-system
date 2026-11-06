#![allow(dead_code)]
//! Shop fixtures shared by the delivery tests (copied from tests/cancel.rs).
use super::{Session, TestApp, TestResponse, app, seed_product, seed_user};
use axum::{body::Body, http::Request};
use jj_retail_api::ledger;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

pub const PW: &str = "correct horse battery";

pub struct Shop {
    pub a: TestApp,
    /// Manager: can cancel (orders.void).
    pub m: Session,
    pub m_id: Uuid,
    /// Cashier: sells and takes payments, can't cancel.
    pub c: Session,
    /// A ₹100 product with 10 in stock.
    pub p: String,
}

pub async fn shop(pool: &PgPool) -> Shop {
    let m_id = seed_user(pool, "boss", PW, &["manager"]).await;
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let m = a.login("boss", PW).await;
    let p = seed_product(pool, m_id, "Plate").await.to_string();
    let id = a
        .post_json(
            "/api/v1/stock-counts",
            json!({}),
            Some(&m.cookie),
            Some(&m.csrf),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = a
        .put_json(
            &format!("/api/v1/stock-counts/{id}/lines"),
            json!({"lines": [{"product_id": p, "counted": 10, "unit_cost": "60"}]}),
            Some(&m.cookie),
            Some(&m.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(
        a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), &m)
            .await
            .status(),
        200
    );
    let c = a.login("cashier", PW).await;
    Shop { a, m, m_id, c, p }
}

pub fn key() -> String {
    format!("key-{}", Uuid::now_v7())
}

pub fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

pub fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}

pub async fn post(a: &TestApp, s: &Session, path: &str, key: &str, body: Value) -> TestResponse {
    a.send(
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

pub async fn sell(sh: &Shop, body: Value) -> Value {
    let r = post(&sh.a, &sh.c, "/api/v1/orders/checkout", &key(), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

/// A pay-later sale of `qty` Plates; a new customer Mary when `customer` is None.
pub async fn on_hand(pool: &PgPool, product: &str) -> i64 {
    sqlx::query_scalar("SELECT on_hand FROM stock_balances WHERE product_id = $1")
        .bind(Uuid::parse_str(product).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn order_owed(pool: &PgPool, order: &Value) -> Decimal {
    let mut c = pool.acquire().await.unwrap();
    ledger::order_owed(&mut c, uuid(&order["id"]))
        .await
        .unwrap()
}

/// Every ledger transaction sums to zero and no receivable is below zero.
pub async fn ledger_is_sound(pool: &PgPool) {
    let unbalanced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT transaction_id FROM ledger_entries
                               GROUP BY transaction_id HAVING SUM(amount) <> 0) x",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unbalanced, 0);
    let negative: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT ref_id FROM ledger_entries WHERE account = 'receivable'
                               GROUP BY ref_id HAVING SUM(amount) < 0) x",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(negative, 0);
}

/// 1 Plate to a new customer with a fixed address, delivered for `fee`, `paid` up front.
pub async fn delivery_sale(sh: &Shop, fee: &str, paid: &str) -> Value {
    let mut body = json!({
        "lines": [{"product_id": sh.p, "quantity": 1}],
        "new_customer": {"phone": "9862012345", "name": "Mary"},
        "delivery": {
            "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
            "address_line": "House 12, Chanmari West", "locality": "Chanmari",
            "landmark": "near the church", "instructions": "call before coming",
            "fee": fee, "driver_pay": "40"
        }
    });
    if paid != "0" {
        body["payments"] = json!([{"method": "cash", "amount": paid}]);
    }
    sell(sh, body).await
}

/// Sum of every ledger entry on `account`.
pub async fn account_total(pool: &PgPool, account: &str) -> Decimal {
    sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM ledger_entries WHERE account = $1")
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}
