//! Return items: owed reduced first, the rest refunded, sellable items restocked (Phase 5b Task 4).
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
    /// Manager: can cancel.
    m: Session,
    /// Cashier: sells and takes returns (returns.create).
    c: Session,
    /// A ₹100 product with 10 in stock.
    p: String,
    /// Another product, also 10 in stock.
    q: String,
}

async fn shop(pool: &PgPool) -> Shop {
    let m_id = seed_user(pool, "boss", PW, &["manager"]).await;
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let m = a.login("boss", PW).await;
    let p = seed_product(pool, m_id, "Plate").await.to_string();
    let q = seed_product(pool, m_id, "Cup").await.to_string();
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
            json!({"lines": [{"product_id": p, "counted": 10, "unit_cost": "60"},
                             {"product_id": q, "counted": 10, "unit_cost": "60"}]}),
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
    Shop { a, m, c, p, q }
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

async fn send(
    a: &TestApp,
    s: &Session,
    path: &str,
    key: Option<&str>,
    body: Value,
) -> TestResponse {
    let mut req = Request::post(path)
        .header("content-type", "application/json")
        .header("cookie", &s.cookie)
        .header("x-csrf-token", &s.csrf);
    if let Some(k) = key {
        req = req.header("idempotency-key", k);
    }
    a.send(req.body(Body::from(body.to_string())).unwrap())
        .await
}

async fn sell(sh: &Shop, body: Value) -> Value {
    let r = send(&sh.a, &sh.c, "/api/v1/orders/checkout", Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

/// A pay-later sale of `qty` Plates to a new customer, Mary.
async fn credit_sale(sh: &Shop, qty: i64) -> Value {
    sell(
        sh,
        json!({"lines": [{"product_id": sh.p, "quantity": qty}],
               "new_customer": {"phone": "9862012345", "name": "Mary"}}),
    )
    .await
}

/// The order's line ids, in line order.
async fn line_ids(pool: &PgPool, order: &Value) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT id FROM order_lines WHERE order_id = $1 ORDER BY line_no")
        .bind(uuid(&order["id"]))
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn give_back(sh: &Shop, order: &Value, key: Option<&str>, body: Value) -> TestResponse {
    send(
        &sh.a,
        &sh.c,
        &format!("/api/v1/orders/{}/returns", order["id"].as_str().unwrap()),
        key,
        body,
    )
    .await
}

fn one(line: Uuid, qty: i64, disposition: &str) -> Value {
    json!([{"order_line_id": line, "quantity": qty, "disposition": disposition}])
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

async fn kinds_on(pool: &PgPool, order: &Value, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM ledger_transactions WHERE order_id = $1 AND kind = $2")
        .bind(uuid(&order["id"]))
        .bind(kind)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// (amount, owed_reduced, refunded, refund_method) of every return on the order, oldest first.
async fn returns_of(
    pool: &PgPool,
    order: &Value,
) -> Vec<(Decimal, Decimal, Decimal, Option<String>)> {
    sqlx::query_as(
        "SELECT amount, owed_reduced, refunded, refund_method FROM returns
         WHERE order_id = $1 ORDER BY created_at, id",
    )
    .bind(uuid(&order["id"]))
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Every ledger transaction sums to zero and no receivable is below zero.
async fn ledger_is_sound(pool: &PgPool) {
    let unbalanced = count(
        pool,
        "SELECT count(*) FROM (SELECT transaction_id FROM ledger_entries
                               GROUP BY transaction_id HAVING SUM(amount) <> 0) x",
    )
    .await;
    assert_eq!(unbalanced, 0);
    let negative = count(
        pool,
        "SELECT count(*) FROM (SELECT ref_id FROM ledger_entries WHERE account = 'receivable'
                               GROUP BY ref_id HAVING SUM(amount) < 0) x",
    )
    .await;
    assert_eq!(negative, 0);
}

fn assert_field(r: &TestResponse, field: &str, message: &str) {
    assert_eq!(r.status(), 422, "{:?}", r.json());
    assert_eq!(
        r.json()["error"]["details"]["field"],
        field,
        "{:?}",
        r.json()
    );
    assert_eq!(r.json()["error"]["message"], message, "{:?}", r.json());
}

#[sqlx::test]
async fn return_sellable_item_refunds_and_restocks(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2}],
               "payments": [{"method": "cash", "amount": "200"}]}),
    )
    .await;
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
    let l = line_ids(&pool, &o).await[0];

    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 1, "sellable"), "refund_method": "cash", "note": "  Chipped  "}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["id"], o["id"]);
    let refunds = v["refunds"].as_array().unwrap();
    assert_eq!(refunds.len(), 1);
    assert_eq!(refunds[0]["method"], "cash");
    assert_eq!(refunds[0]["amount"], "100.00");

    assert_eq!(on_hand(&pool, &sh.p).await, 9);
    assert_eq!(
        returns_of(&pool, &o).await,
        [(d("100"), d("0"), d("100"), Some("cash".to_string()))]
    );
    let note: Option<String> = sqlx::query_scalar("SELECT note FROM returns WHERE order_id = $1")
        .bind(uuid(&o["id"]))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(note.as_deref(), Some("Chipped"));
    let line: (i64, String, Decimal) =
        sqlx::query_as("SELECT quantity, disposition, amount FROM return_lines")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(line, (1, "sellable".to_string(), d("100")));
    assert_eq!(kinds_on(&pool, &o, "refund").await, 1);
    assert_eq!(kinds_on(&pool, &o, "return_credit").await, 0);
    let return_id: Uuid = sqlx::query_scalar("SELECT id FROM returns")
        .fetch_one(&pool)
        .await
        .unwrap();
    let key_ok: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ledger_transactions WHERE idempotency_key = $1 AND method = 'cash'",
    )
    .bind(format!("return:{return_id}:refund"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(key_ok, 1);
    let sales_returns: Decimal = sqlx::query_scalar(
        "SELECT SUM(amount) FROM ledger_entries WHERE account = 'sales_returns'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sales_returns, d("100"));
    let moves: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements
         WHERE movement_type = 'sale_return' AND source_type = 'return' AND source_id = $1
           AND quantity = 1 AND unit_cost IS NULL",
    )
    .bind(return_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(moves, 1);
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'order.return' AND entity_id = $1",
    )
    .bind(uuid(&o["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn return_damaged_item_does_not_restock(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2}],
               "payments": [{"method": "upi", "amount": "200"}]}),
    )
    .await;
    let l = line_ids(&pool, &o).await[0];
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 2, "damaged"), "refund_method": "upi"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM stock_movements WHERE movement_type = 'sale_return'"
        )
        .await,
        0
    );
    assert_eq!(
        returns_of(&pool, &o).await,
        [(d("200"), d("0"), d("200"), Some("upi".to_string()))]
    );
    assert_eq!(r.json()["refunds"][0]["method"], "upi");
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn return_on_unpaid_sale_only_reduces_owed(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, 2).await;
    let l = line_ids(&pool, &o).await[0];
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 1, "sellable")}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["owed"], "100.00");
    assert_eq!(r.json()["refunds"], json!([]));
    assert_eq!(order_owed(&pool, &o).await, d("100"));
    assert_eq!(kinds_on(&pool, &o, "refund").await, 0);
    assert_eq!(kinds_on(&pool, &o, "return_credit").await, 1);
    // The credit carries the customer too.
    let with_customer: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ledger_transactions
         WHERE order_id = $1 AND kind = 'return_credit' AND customer_id = $2
           AND method = 'internal'",
    )
    .bind(uuid(&o["id"]))
    .bind(uuid(&o["customer"]["id"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(with_customer, 1);
    assert_eq!(
        returns_of(&pool, &o).await,
        [(d("100"), d("100"), d("0"), None)]
    );
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn return_on_part_paid_sale_reduces_owed_then_refunds(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 3}],
               "new_customer": {"phone": "9862012345", "name": "Mary"},
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await;
    assert_eq!(order_owed(&pool, &o).await, d("200"));
    let l = line_ids(&pool, &o).await[0];
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 3, "sellable"), "refund_method": "cash"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(
        returns_of(&pool, &o).await,
        [(d("300"), d("200"), d("100"), Some("cash".to_string()))]
    );
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    assert_eq!(kinds_on(&pool, &o, "return_credit").await, 1);
    assert_eq!(kinds_on(&pool, &o, "refund").await, 1);
    assert_eq!(r.json()["refunds"][0]["amount"], "100.00");
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn partial_returns_add_up_to_net(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 3, "line_discount": "200"}],
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await;
    assert_eq!(o["total"], "100.00");
    let l = line_ids(&pool, &o).await[0];
    for _ in 0..3 {
        let r = give_back(
            &sh,
            &o,
            Some(&key()),
            json!({"lines": one(l, 1, "sellable"), "refund_method": "cash"}),
        )
        .await;
        assert_eq!(r.status(), 201, "{:?}", r.json());
    }
    let amounts: Vec<Decimal> = returns_of(&pool, &o).await.iter().map(|r| r.0).collect();
    assert_eq!(amounts, [d("33.33"), d("33.34"), d("33.33")]);
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 1, "sellable"), "refund_method": "cash"}),
    )
    .await;
    assert_field(&r, "lines[0].quantity", "Only 0 left to return.");
    let cash: Decimal =
        sqlx::query_scalar("SELECT SUM(amount) FROM ledger_entries WHERE account = 'cash'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cash, Decimal::ZERO);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn cannot_return_more_than_left(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, 2).await;
    let l = line_ids(&pool, &o).await[0];
    assert_eq!(
        give_back(
            &sh,
            &o,
            Some(&key()),
            json!({"lines": one(l, 1, "sellable")})
        )
        .await
        .status(),
        201
    );
    for qty in [2, 0, -1] {
        let r = give_back(
            &sh,
            &o,
            Some(&key()),
            json!({"lines": one(l, qty, "sellable")}),
        )
        .await;
        assert_field(&r, "lines[0].quantity", "Only 1 left to return.");
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM return_lines").await, 1);
    assert_eq!(kinds_on(&pool, &o, "return_credit").await, 1);
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
}

#[sqlx::test]
async fn line_from_another_order_is_rejected(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, 1).await;
    let other = credit_sale(&sh, 1).await;
    let foreign = line_ids(&pool, &other).await[0];
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(foreign, 1, "sellable")}),
    )
    .await;
    assert_field(
        &r,
        "lines[0].order_line_id",
        "This item isn't on this sale.",
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
}

#[sqlx::test]
async fn refund_method_required_when_refunding(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 1}],
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await;
    let l = line_ids(&pool, &o).await[0];
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 1, "sellable")}),
    )
    .await;
    assert_field(&r, "refund_method", "Choose cash or UPI for the refund.");
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(l, 1, "sellable"), "refund_method": "card"}),
    )
    .await;
    assert_field(&r, "refund_method", "Choose cash or UPI for the refund.");
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
}

#[sqlx::test]
async fn return_checks_input(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2},
                         {"product_id": sh.q, "quantity": 1, "unit_price": "0"}],
               "payments": [{"method": "cash", "amount": "200"}]}),
    )
    .await;
    let ids = line_ids(&pool, &o).await;
    let r = give_back(&sh, &o, Some(&key()), json!({"lines": []})).await;
    assert_field(&r, "lines", "Add at least one item to return.");
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": [{"order_line_id": ids[0], "quantity": 1, "disposition": "sellable"},
                         {"order_line_id": ids[0], "quantity": 1, "disposition": "sellable"}],
               "refund_method": "cash"}),
    )
    .await;
    assert_field(
        &r,
        "lines[1].order_line_id",
        "This item is already on the return. Change that line instead.",
    );
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(ids[0], 1, "broken"), "refund_method": "cash"}),
    )
    .await;
    assert_field(&r, "lines[0].disposition", "Pick sellable or damaged.");
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(ids[0], 1, "sellable"), "refund_method": "cash",
               "note": "x".repeat(501)}),
    )
    .await;
    assert_field(&r, "note", "Keep the note to 500 characters or fewer.");
    // A free item alone has nothing to give back.
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": one(ids[1], 1, "sellable")}),
    )
    .await;
    assert_field(
        &r,
        "lines",
        "These items were free, so there's nothing to return.",
    );
    // No key.
    let r = give_back(
        &sh,
        &o,
        None,
        json!({"lines": one(ids[0], 1, "sellable"), "refund_method": "cash"}),
    )
    .await;
    assert_eq!(r.status(), 400);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");
    // Unknown order.
    let missing = json!({"id": Uuid::now_v7().to_string()});
    let r = give_back(
        &sh,
        &missing,
        Some(&key()),
        json!({"lines": one(ids[0], 1, "sellable"), "refund_method": "cash"}),
    )
    .await;
    assert_eq!(r.status(), 404);
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
    // A free item alongside a paid one comes back too, at ₹0.
    let r = give_back(
        &sh,
        &o,
        Some(&key()),
        json!({"lines": [{"order_line_id": ids[0], "quantity": 1, "disposition": "sellable"},
                         {"order_line_id": ids[1], "quantity": 1, "disposition": "sellable"}],
               "refund_method": "cash", "note": "   "}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
    assert_eq!(on_hand(&pool, &sh.q).await, 10);
    let note: Option<String> = sqlx::query_scalar("SELECT note FROM returns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(note, None);
}

#[sqlx::test]
async fn preview_writes_nothing(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 3}],
               "new_customer": {"phone": "9862012345", "name": "Mary"},
               "payments": [{"method": "cash", "amount": "100"}]}),
    )
    .await;
    let l = line_ids(&pool, &o).await[0];
    let before_ledger = count(&pool, "SELECT count(*) FROM ledger_transactions").await;
    // No key and no refund method: the dialog asks for the method after showing the refund.
    let r = give_back(
        &sh,
        &o,
        None,
        json!({"lines": one(l, 3, "sellable"), "preview": true}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(
        r.json(),
        json!({"amount": "300.00", "owed_reduced": "200.00", "refund": "100.00",
               "lines": [{"order_line_id": l, "quantity": 3, "amount": "300.00"}]})
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM return_lines").await, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ledger_transactions").await,
        before_ledger
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM audit_logs WHERE action = 'order.return'"
        )
        .await,
        0
    );
    assert_eq!(on_hand(&pool, &sh.p).await, 7);
    // A preview still checks what is left.
    let r = give_back(
        &sh,
        &o,
        None,
        json!({"lines": one(l, 4, "sellable"), "preview": true}),
    )
    .await;
    assert_field(&r, "lines[0].quantity", "Only 3 left to return.");
}

#[sqlx::test]
async fn retry_with_same_key_returns_once(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sell(
        &sh,
        json!({"lines": [{"product_id": sh.p, "quantity": 2}],
               "payments": [{"method": "cash", "amount": "200"}]}),
    )
    .await;
    let l = line_ids(&pool, &o).await[0];
    let k = key();
    let body = json!({"lines": one(l, 1, "sellable"), "refund_method": "cash"});
    let first = give_back(&sh, &o, Some(&k), body.clone()).await;
    assert_eq!(first.status(), 201, "{:?}", first.json());
    let second = give_back(&sh, &o, Some(&k), body).await;
    assert_eq!(second.status(), 200, "{:?}", second.json());
    assert_eq!(second.json()["refunds"], first.json()["refunds"]);
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 1);
    assert_eq!(kinds_on(&pool, &o, "refund").await, 1);
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
    // Same key, different body.
    let r = give_back(
        &sh,
        &o,
        Some(&k),
        json!({"lines": one(l, 1, "damaged"), "refund_method": "cash"}),
    )
    .await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_CONFLICT");
}

#[sqlx::test]
async fn return_on_cancelled_sale_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, 2).await;
    let l = line_ids(&pool, &o).await[0];
    let r = send(
        &sh.a,
        &sh.m,
        &format!("/api/v1/orders/{}/cancel", o["id"].as_str().unwrap()),
        Some(&key()),
        json!({}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    for preview in [false, true] {
        let r = give_back(
            &sh,
            &o,
            Some(&key()),
            json!({"lines": one(l, 1, "sellable"), "preview": preview}),
        )
        .await;
        assert_eq!(r.status(), 409, "{:?}", r.json());
        assert_eq!(r.json()["error"]["code"], "ALREADY_CANCELLED");
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
}

#[sqlx::test]
async fn return_needs_returns_create(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = credit_sale(&sh, 1).await;
    let l = line_ids(&pool, &o).await[0];
    seed_user(&pool, "auditor", PW, &["auditor"]).await;
    let aud = sh.a.login("auditor", PW).await;
    for preview in [false, true] {
        let r = send(
            &sh.a,
            &aud,
            &format!("/api/v1/orders/{}/returns", o["id"].as_str().unwrap()),
            Some(&key()),
            json!({"lines": one(l, 1, "sellable"), "preview": preview}),
        )
        .await;
        assert_eq!(r.status(), 403, "{:?}", r.json());
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);
}
