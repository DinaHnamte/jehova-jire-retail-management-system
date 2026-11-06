//! Cancel a sale: stock back, sale reversed, what was paid refunded by method (Phase 5b Task 3).
mod common;
use axum::{body::Body, http::Request};
use common::{Session, TestApp, TestResponse, app, seed_product, seed_user};
use jj_retail_api::ledger;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Shop {
    a: TestApp,
    /// Manager: can cancel (orders.void).
    m: Session,
    m_id: Uuid,
    /// Cashier: sells and takes payments, can't cancel.
    c: Session,
    /// A ₹100 product with 10 in stock.
    p: String,
}

async fn shop(pool: &PgPool) -> Shop {
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

fn key() -> String {
    format!("key-{}", Uuid::now_v7())
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}

async fn post(a: &TestApp, s: &Session, path: &str, key: &str, body: Value) -> TestResponse {
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

async fn sell(sh: &Shop, body: Value) -> Value {
    let r = post(&sh.a, &sh.c, "/api/v1/orders/checkout", &key(), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

/// A pay-later sale of `qty` Plates; a new customer Mary when `customer` is None.
async fn credit_sale(sh: &Shop, customer: Option<&str>, qty: i64) -> Value {
    let mut body = json!({"lines": [{"product_id": sh.p, "quantity": qty}]});
    match customer {
        None => body["new_customer"] = json!({"phone": "9862012345", "name": "Mary"}),
        Some(id) => body["customer_id"] = json!(id),
    }
    sell(sh, body).await
}

async fn cancel(sh: &Shop, s: &Session, order: &Value, key: &str, body: Value) -> TestResponse {
    post(
        &sh.a,
        s,
        &format!("/api/v1/orders/{}/cancel", order["id"].as_str().unwrap()),
        key,
        body,
    )
    .await
}

async fn on_hand(pool: &PgPool, product: &str) -> i64 {
    sqlx::query_scalar("SELECT on_hand FROM stock_balances WHERE product_id = $1")
        .bind(Uuid::parse_str(product).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn order_owed(pool: &PgPool, order: &Value) -> Decimal {
    let mut c = pool.acquire().await.unwrap();
    ledger::order_owed(&mut c, uuid(&order["id"]))
        .await
        .unwrap()
}

async fn customer_owes(pool: &PgPool, customer: &str) -> Decimal {
    let mut c = pool.acquire().await.unwrap();
    ledger::customer_owes(&mut c, Uuid::parse_str(customer).unwrap())
        .await
        .unwrap()
}

async fn kinds_on(pool: &PgPool, order: &Value, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM ledger_transactions WHERE order_id = $1 AND kind = $2")
        .bind(uuid(&order["id"]))
        .bind(kind)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every ledger transaction sums to zero and no receivable is below zero.
async fn ledger_is_sound(pool: &PgPool) {
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

#[sqlx::test]
async fn cancel_paid_sale_refunds_by_method_and_restores_stock(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2}],
               "payments": [{"method": "cash", "amount": "150"},
                            {"method": "upi", "amount": "50"}]}),
    )
    .await;
    assert_eq!(on_hand(&pool, &sh.p).await, 8);

    let r = cancel(&sh, &sh.m, &o, &key(), json!({"reason": "  Wrong item  "})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["status"], "cancelled");
    assert_eq!(v["owed"], "0.00");
    let refunds: Vec<(String, String)> = v["refunds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["method"].as_str().unwrap().to_string(),
                x["amount"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        refunds,
        [
            ("cash".to_string(), "150.00".to_string()),
            ("upi".to_string(), "50.00".to_string())
        ]
    );
    assert!(v["refunds"][0]["occurred_at"].is_string());

    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    for account in ["cash", "upi"] {
        let net: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(e.amount), 0) FROM ledger_entries e
             JOIN ledger_transactions t ON t.id = e.transaction_id
             WHERE t.order_id = $1 AND e.account = $2",
        )
        .bind(uuid(&o["id"]))
        .bind(account)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(net, Decimal::ZERO, "{account}");
    }

    // The cancel reverses the sale transaction.
    let reverses: Option<Uuid> = sqlx::query_scalar(
        "SELECT reverses_id FROM ledger_transactions WHERE order_id = $1 AND kind = 'cancel'",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    let sale: Uuid = sqlx::query_scalar(
        "SELECT id FROM ledger_transactions WHERE order_id = $1 AND kind = 'sale'",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reverses, Some(sale));

    let row: (String, Option<Uuid>, Option<String>, bool) = sqlx::query_as(
        "SELECT status, cancelled_by, cancel_reason, cancelled_at IS NOT NULL
         FROM orders WHERE id = $1",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "cancelled".to_string(),
            Some(sh.m_id),
            Some("Wrong item".to_string()),
            true
        )
    );
    let movements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements
         WHERE movement_type = 'sale_cancel' AND source_type = 'order' AND source_id = $1
           AND quantity = 2 AND unit_cost IS NULL",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(movements, 1);
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'order.cancel' AND entity_id = $1",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn cancel_part_paid_sale_after_later_payment(pool: PgPool) {
    let sh = shop(&pool).await;
    let a = credit_sale(&sh, None, 2).await;
    let cid = a["customer"]["id"].as_str().unwrap().to_string();
    let b = credit_sale(&sh, Some(&cid), 2).await;
    let r = post(
        &sh.a,
        &sh.c,
        &format!("/api/v1/customers/{cid}/payments"),
        &key(),
        json!({"method": "upi", "amount": "300"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(order_owed(&pool, &a).await, Decimal::ZERO);
    assert_eq!(order_owed(&pool, &b).await, d("100"));

    let r = cancel(&sh, &sh.m, &b, &key(), json!({})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["refunds"].as_array().unwrap().len(), 1);
    assert_eq!(v["refunds"][0]["method"], "upi");
    assert_eq!(v["refunds"][0]["amount"], "100.00");
    assert_eq!(order_owed(&pool, &a).await, Decimal::ZERO);
    assert_eq!(order_owed(&pool, &b).await, Decimal::ZERO);
    assert_eq!(customer_owes(&pool, &cid).await, Decimal::ZERO);
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn cancel_unpaid_sale_refunds_nothing(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 2).await;
    let cid = o["customer"]["id"].as_str().unwrap().to_string();
    assert_eq!(customer_owes(&pool, &cid).await, d("200"));
    let r = cancel(&sh, &sh.m, &o, &key(), json!({})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(r.json()["refunds"], json!([]));
    assert_eq!(kinds_on(&pool, &o, "refund").await, 0);
    assert_eq!(kinds_on(&pool, &o, "cancel").await, 1);
    assert_eq!(customer_owes(&pool, &cid).await, Decimal::ZERO);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn cancel_zero_total_sale(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 3, "unit_price": "0"}]}),
    )
    .await;
    assert_eq!(o["total"], "0.00");
    assert_eq!(on_hand(&pool, &sh.p).await, 7);
    let r = cancel(&sh, &sh.m, &o, &key(), json!({})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(r.json()["status"], "cancelled");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
}

#[sqlx::test]
async fn cancel_twice_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 1).await;
    assert_eq!(
        cancel(&sh, &sh.m, &o, &key(), json!({})).await.status(),
        200
    );
    let r = cancel(&sh, &sh.m, &o, &key(), json!({})).await;
    assert_eq!(r.status(), 409, "{:?}", r.json());
    assert_eq!(r.json()["error"]["code"], "ALREADY_CANCELLED");
    assert_eq!(
        r.json()["error"]["message"],
        "This sale is already cancelled."
    );
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
}

#[sqlx::test]
async fn retry_with_same_key_cancels_once(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2}],
               "payments": [{"method": "cash", "amount": "200"}]}),
    )
    .await;
    let k = key();
    let first = cancel(&sh, &sh.m, &o, &k, json!({"reason": "Typo"})).await;
    assert_eq!(first.status(), 200, "{:?}", first.json());
    let second = cancel(&sh, &sh.m, &o, &k, json!({"reason": "Typo"})).await;
    assert_eq!(second.status(), 200, "{:?}", second.json());
    assert_eq!(second.json()["status"], "cancelled");
    assert_eq!(second.json()["refunds"], first.json()["refunds"]);
    assert_eq!(kinds_on(&pool, &o, "cancel").await, 1);
    assert_eq!(kinds_on(&pool, &o, "refund").await, 1);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    // Same key, different body.
    let r = cancel(&sh, &sh.m, &o, &k, json!({"reason": "Other"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_CONFLICT");
}

#[sqlx::test]
async fn cancel_after_return_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 2).await;
    sqlx::query(
        "INSERT INTO returns (id, order_id, amount, owed_reduced, refunded, created_by)
         VALUES ($1, $2, 100, 100, 0, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(uuid(&o["id"]))
    .bind(sh.m_id)
    .execute(&pool)
    .await
    .unwrap();
    let r = cancel(&sh, &sh.m, &o, &key(), json!({})).await;
    assert_eq!(r.status(), 409, "{:?}", r.json());
    assert_eq!(r.json()["error"]["code"], "HAS_RETURNS");
    assert_eq!(
        r.json()["error"]["message"],
        "Some items were already returned. Return the rest instead."
    );
    assert_eq!(kinds_on(&pool, &o, "cancel").await, 0);
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
}

#[sqlx::test]
async fn cancel_needs_orders_void(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 1).await;
    let r = cancel(&sh, &sh.c, &o, &key(), json!({})).await;
    assert_eq!(r.status(), 403, "{:?}", r.json());
    assert_eq!(kinds_on(&pool, &o, "cancel").await, 0);
}

#[sqlx::test]
async fn cancel_checks_input(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, None, 1).await;
    let r = cancel(&sh, &sh.m, &o, &key(), json!({"reason": "x".repeat(501)})).await;
    assert_eq!(r.status(), 422, "{:?}", r.json());
    assert_eq!(r.json()["error"]["details"]["field"], "reason");
    // No key.
    let r =
        sh.a.send(
            Request::post(format!(
                "/api/v1/orders/{}/cancel",
                o["id"].as_str().unwrap()
            ))
            .header("content-type", "application/json")
            .header("cookie", &sh.m.cookie)
            .header("x-csrf-token", &sh.m.csrf)
            .body(Body::from("{}"))
            .unwrap(),
        )
        .await;
    assert_eq!(r.status(), 400);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");
    // Unknown order.
    let missing = json!({"id": Uuid::now_v7().to_string()});
    assert_eq!(
        cancel(&sh, &sh.m, &missing, &key(), json!({}))
            .await
            .status(),
        404
    );
    // A blank reason is stored as NULL.
    assert_eq!(
        cancel(&sh, &sh.m, &o, &key(), json!({"reason": "   "}))
            .await
            .status(),
        200
    );
    let reason: Option<String> =
        sqlx::query_scalar("SELECT cancel_reason FROM orders WHERE id = $1")
            .bind(uuid(&o["id"]))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, None);
}

#[sqlx::test]
async fn concurrent_payment_and_cancel_keep_owed_in_range(pool: PgPool) {
    let sh = shop(&pool).await;
    let first = credit_sale(&sh, None, 2).await;
    let cid = first["customer"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/v1/customers/{cid}/payments");
    // `join!` polls in argument order, so each round gives the other request a head start.
    for payment_first in [true, false] {
        let o = if payment_first {
            first.clone()
        } else {
            credit_sale(&sh, Some(&cid), 2).await
        };
        let (pay_key, cancel_key) = (key(), key());
        let pay = post(
            &sh.a,
            &sh.c,
            &path,
            &pay_key,
            json!({"method": "cash", "amount": "200"}),
        );
        let can = cancel(&sh, &sh.m, &o, &cancel_key, json!({}));
        let (pay, can) = if payment_first {
            tokio::join!(pay, can)
        } else {
            let (c, p) = tokio::join!(can, pay);
            (p, c)
        };
        // Either the payment went first (and was refunded) or it found nothing owed.
        assert!(
            pay.status() == 201 || pay.status() == 422,
            "{:?}",
            pay.json()
        );
        assert_eq!(can.status(), 200, "{:?}", can.json());
        assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
        assert_eq!(customer_owes(&pool, &cid).await, Decimal::ZERO);
        ledger_is_sound(&pool).await;
        // Cash nets to zero either way.
        let cash: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM ledger_entries WHERE account = 'cash'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cash, Decimal::ZERO);
    }
}
