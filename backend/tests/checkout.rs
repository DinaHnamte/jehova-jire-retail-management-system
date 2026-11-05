//! One-call checkout: numbering, stock, money ledger, idempotency (Phase 5a Task 6).
mod common;
use axum::{body::Body, http::Request};
use chrono::{TimeZone, Utc};
use common::{Session, TestApp, TestResponse, app, seed_product, seed_user};
use jj_retail_api::{ledger, orders::domain::fiscal_year, orders::numbers::next_number};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Shop {
    a: TestApp,
    s: Session,
    /// Two products at ₹100 with 10 in stock each.
    p: Vec<String>,
}

/// Post an opening count for `lines` (product id, counted) at cost 60, as the manager.
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

/// Manager seeds two ₹100 products with `stock` units each; a cashier is logged in.
async fn shop_with(pool: &PgPool, stock: i64) -> Shop {
    let boss = seed_user(pool, "boss", PW, &["manager"]).await;
    seed_user(pool, "cashier", PW, &["cashier"]).await;
    let a = app(pool.clone());
    let m = a.login("boss", PW).await;
    let p1 = seed_product(pool, boss, "Plate").await.to_string();
    let p2 = seed_product(pool, boss, "Bowl").await.to_string();
    opening(&a, &m, &[(&p1, stock), (&p2, stock)]).await;
    let s = a.login("cashier", PW).await;
    Shop {
        a,
        s,
        p: vec![p1, p2],
    }
}

async fn shop(pool: &PgPool) -> Shop {
    shop_with(pool, 10).await
}

async fn checkout(a: &TestApp, s: &Session, key: Option<&str>, body: Value) -> TestResponse {
    let mut b = Request::post("/api/v1/orders/checkout")
        .header("content-type", "application/json")
        .header("cookie", &s.cookie)
        .header("x-csrf-token", &s.csrf);
    if let Some(k) = key {
        b = b.header("idempotency-key", k);
    }
    a.send(b.body(Body::from(body.to_string())).unwrap()).await
}

fn key() -> String {
    format!("key-{}", Uuid::now_v7())
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

async fn on_hand(pool: &PgPool, product: &str) -> i64 {
    sqlx::query_scalar("SELECT on_hand FROM stock_balances WHERE product_id = $1")
        .bind(Uuid::parse_str(product).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

fn cash_sale(p: &str, qty: i64, cash: &str) -> Value {
    json!({"lines": [{"product_id": p, "quantity": qty}],
           "payments": [{"method": "cash", "amount": cash}]})
}

#[sqlx::test]
async fn cash_sale_takes_stock_posts_money_and_numbers(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = checkout(&sh.a, &sh.s, Some(&key()), cash_sale(&sh.p[0], 2, "200")).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let v = r.json();
    let fy = fiscal_year(Utc::now());
    assert_eq!(v["number"], format!("JJ/{fy}/000001"));
    assert_eq!(v["total"], "200.00");
    assert_eq!(v["paid"], "200.00");
    assert_eq!(v["owed"], "0.00");
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 8);
    let id = Uuid::parse_str(v["id"].as_str().unwrap()).unwrap();
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM ledger_transactions WHERE order_id = $1 ORDER BY kind DESC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(kinds, ["sale", "payment"]);
    let mut c = pool.acquire().await.unwrap();
    assert_eq!(ledger::order_owed(&mut c, id).await.unwrap(), Decimal::ZERO);
    // The line snapshots the average cost of the stock it took.
    let cost: Option<Decimal> =
        sqlx::query_scalar("SELECT unit_cost FROM order_lines WHERE order_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cost, Some(d("60")));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM audit_logs WHERE action = 'order.checkout'"
        )
        .await,
        1
    );
}

#[sqlx::test]
async fn server_prices_ignore_client_totals(pool: PgPool) {
    let sh = shop(&pool).await;
    let mut body = cash_sale(&sh.p[0], 2, "200");
    body["total"] = json!("1");
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["total"], "200.00");
}

#[sqlx::test]
async fn price_edit_and_discounts_are_snapshotted(pool: PgPool) {
    let sh = shop(&pool).await;
    let body = json!({
        "lines": [{"product_id": sh.p[0], "quantity": 2, "unit_price": "90", "line_discount": "10"}],
        "order_discount": "20",
        "payments": [{"method": "cash", "amount": "150"}]
    });
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["total"], "150.00");
    let id = Uuid::parse_str(r.json()["id"].as_str().unwrap()).unwrap();
    let (list, unit, net, name): (Decimal, Decimal, Decimal, String) = sqlx::query_as(
        "SELECT list_price, unit_price, net_amount, product_name FROM order_lines WHERE order_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(list.to_string(), "100.00");
    assert_eq!(unit.to_string(), "90.00");
    assert_eq!(net.to_string(), "150.00");
    assert_eq!(name, "Plate");
}

#[sqlx::test]
async fn pay_later_needs_a_customer(pool: PgPool) {
    let sh = shop(&pool).await;
    let lines = json!([{"product_id": sh.p[0], "quantity": 2}]);
    let r = checkout(&sh.a, &sh.s, Some(&key()), json!({"lines": lines})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "CUSTOMER_REQUIRED");
    let r = checkout(
        &sh.a,
        &sh.s,
        Some(&key()),
        json!({"lines": lines, "new_customer": {"phone": "+91 98620-12345", "name": "Mary"}}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["owed"], v["total"]);
    assert_eq!(v["owed"], "200.00");
    let cid: Uuid = sqlx::query_scalar("SELECT id FROM customers WHERE phone = '9862012345'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(v["customer"]["id"], cid.to_string());
}

#[sqlx::test]
async fn part_payment_leaves_owed(pool: PgPool) {
    let sh = shop(&pool).await;
    let body = json!({
        "lines": [{"product_id": sh.p[0], "quantity": 2}],
        "new_customer": {"phone": "9862012345"},
        "payments": [{"method": "cash", "amount": "50"}, {"method": "upi", "amount": "50"}]
    });
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["paid"], "100.00");
    assert_eq!(r.json()["owed"], "100.00");
    let rows: Vec<(String, Decimal)> = sqlx::query_as(
        "SELECT account, amount FROM ledger_entries WHERE account IN ('cash', 'upi') ORDER BY account",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [
            ("cash".to_string(), d("50.00")),
            ("upi".to_string(), d("50.00"))
        ]
    );
    // Each payment transaction carries one method.
    let methods: Vec<String> = sqlx::query_scalar(
        "SELECT method FROM ledger_transactions WHERE kind = 'payment' ORDER BY method",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(methods, ["cash", "upi"]);
}

#[sqlx::test]
async fn overpayment_is_refused(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = checkout(&sh.a, &sh.s, Some(&key()), cash_sale(&sh.p[0], 2, "250")).await;
    assert_eq!(r.status(), 422);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "OVERPAYMENT");
    assert_eq!(e["details"]["max"], "200.00");
    assert_eq!(
        e["message"],
        "Paid is more than the total. Enter ₹200.00 or less."
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 0);
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 10);
}

#[sqlx::test]
async fn zero_total_sale_posts_no_money(pool: PgPool) {
    let sh = shop(&pool).await;
    let body = json!({"lines": [{"product_id": sh.p[0], "quantity": 2}], "order_discount": "200"});
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["total"], "0.00");
    assert_eq!(r.json()["owed"], "0.00");
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ledger_transactions").await,
        0
    );
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 8);
}

#[sqlx::test]
async fn insufficient_stock_writes_nothing(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = checkout(&sh.a, &sh.s, Some(&key()), cash_sale(&sh.p[0], 11, "1100")).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "INSUFFICIENT_STOCK");
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM order_lines").await, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM document_sequences").await,
        0
    );
    let r = checkout(&sh.a, &sh.s, Some(&key()), cash_sale(&sh.p[0], 1, "100")).await;
    assert_eq!(r.status(), 201);
    assert!(r.json()["number"].as_str().unwrap().ends_with("/000001"));
}

#[sqlx::test]
async fn retry_with_same_key_makes_one_sale(pool: PgPool) {
    let sh = shop(&pool).await;
    let k = key();
    let first = checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 2, "200")).await;
    assert_eq!(first.status(), 201);
    let again = checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 2, "200")).await;
    assert_eq!(again.status(), 200);
    assert_eq!(again.json()["id"], first.json()["id"]);
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 8);
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 1);
}

#[sqlx::test]
async fn same_key_different_body_conflicts(pool: PgPool) {
    let sh = shop(&pool).await;
    let k = key();
    let first = checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 2, "200")).await;
    assert_eq!(first.status(), 201);
    let r = checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 1, "100")).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_CONFLICT");
}

#[sqlx::test]
async fn missing_key_is_400(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = checkout(&sh.a, &sh.s, None, cash_sale(&sh.p[0], 2, "200")).await;
    assert_eq!(r.status(), 400);
    assert_eq!(r.json()["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 0);
}

#[sqlx::test]
async fn concurrent_checkouts_for_last_unit_one_wins(pool: PgPool) {
    let sh = shop_with(&pool, 1).await;
    let (k1, k2) = (key(), key());
    let (r1, r2) = tokio::join!(
        checkout(&sh.a, &sh.s, Some(&k1), cash_sale(&sh.p[0], 1, "100")),
        checkout(&sh.a, &sh.s, Some(&k2), cash_sale(&sh.p[0], 1, "100")),
    );
    let mut statuses = [r1.status(), r2.status()];
    statuses.sort();
    assert_eq!(statuses, [201, 422]);
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 1);
}

#[sqlx::test]
async fn concurrent_same_key_makes_one_sale(pool: PgPool) {
    let sh = shop(&pool).await;
    let k = key();
    let (r1, r2) = tokio::join!(
        checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 2, "200")),
        checkout(&sh.a, &sh.s, Some(&k), cash_sale(&sh.p[0], 2, "200")),
    );
    assert!(r1.status() / 100 == 2, "{:?}", r1.json());
    assert!(r2.status() / 100 == 2, "{:?}", r2.json());
    assert_eq!(r1.json()["id"], r2.json()["id"]);
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 1);
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 8);
}

#[sqlx::test]
async fn concurrent_checkouts_get_gapless_numbers(pool: PgPool) {
    let sh = shop(&pool).await;
    let ks: Vec<String> = (0..5).map(|_| key()).collect();
    let body = || cash_sale(&sh.p[0], 1, "100");
    let rs = tokio::join!(
        checkout(&sh.a, &sh.s, Some(&ks[0]), body()),
        checkout(&sh.a, &sh.s, Some(&ks[1]), body()),
        checkout(&sh.a, &sh.s, Some(&ks[2]), body()),
        checkout(&sh.a, &sh.s, Some(&ks[3]), body()),
        checkout(&sh.a, &sh.s, Some(&ks[4]), body()),
    );
    let mut nums: Vec<String> = [rs.0, rs.1, rs.2, rs.3, rs.4]
        .iter()
        .map(|r| {
            assert_eq!(r.status(), 201, "{:?}", r.json());
            let n = r.json()["number"].as_str().unwrap().to_string();
            n.rsplit('/').next().unwrap().to_string()
        })
        .collect();
    nums.sort();
    assert_eq!(nums, ["000001", "000002", "000003", "000004", "000005"]);
    assert_eq!(on_hand(&pool, &sh.p[0]).await, 5);
}

#[sqlx::test]
async fn numbering_restarts_each_fiscal_year(pool: PgPool) {
    let mut c = pool.acquire().await.unwrap();
    let fy25 = Utc.with_ymd_and_hms(2026, 3, 31, 18, 29, 59).unwrap();
    let fy26 = Utc.with_ymd_and_hms(2026, 3, 31, 18, 30, 0).unwrap();
    assert_eq!(
        next_number(&mut c, "receipt", fy25).await.unwrap(),
        "JJ/2025-26/000001"
    );
    assert_eq!(
        next_number(&mut c, "receipt", fy25).await.unwrap(),
        "JJ/2025-26/000002"
    );
    assert_eq!(
        next_number(&mut c, "receipt", fy26).await.unwrap(),
        "JJ/2026-27/000001"
    );
}

#[sqlx::test]
async fn existing_phone_reuses_customer(pool: PgPool) {
    let sh = shop(&pool).await;
    let made =
        sh.a.post_json(
            "/api/v1/customers",
            json!({"phone": "9862012345", "name": "Mary"}),
            Some(&sh.s.cookie),
            Some(&sh.s.csrf),
        )
        .await;
    assert_eq!(made.status(), 201);
    let body = json!({"lines": [{"product_id": sh.p[0], "quantity": 1}],
                      "new_customer": {"phone": "09862012345"}});
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["customer"]["id"], made.json()["id"]);
    assert_eq!(count(&pool, "SELECT count(*) FROM customers").await, 1);
}

#[sqlx::test]
async fn cashier_without_confirm_is_403(pool: PgPool) {
    let sh = shop(&pool).await;
    seed_user(&pool, "viewer", PW, &["auditor"]).await;
    let v = sh.a.login("viewer", PW).await;
    let r = checkout(&sh.a, &v, Some(&key()), cash_sale(&sh.p[0], 1, "100")).await;
    assert_eq!(r.status(), 403);
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 0);
}

async fn field_of(sh: &Shop, body: Value) -> (String, String) {
    let r = checkout(&sh.a, &sh.s, Some(&key()), body).await;
    assert_eq!(r.status(), 422, "{:?}", r.json());
    let e = r.json()["error"].clone();
    (
        e["details"]["field"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        e["message"].as_str().unwrap().to_string(),
    )
}

#[sqlx::test]
async fn bad_inputs_name_their_field(pool: PgPool) {
    let sh = shop(&pool).await;
    let p = &sh.p[0];
    let cases = [
        (json!({"lines": []}), "lines"),
        (
            json!({"lines": [{"product_id": p, "quantity": 0}]}),
            "lines[0].quantity",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1_000_001}]}),
            "lines[0].quantity",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1, "unit_price": "-5"}]}),
            "lines[0].unit_price",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1, "unit_price": "1.234"}]}),
            "lines[0].unit_price",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1, "line_discount": "-1"}]}),
            "lines[0].line_discount",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1, "line_discount": "101"}]}),
            "lines[0].line_discount",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "order_discount": "-1"}),
            "order_discount",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "order_discount": "101"}),
            "order_discount",
        ),
        (
            json!({"lines": [{"product_id": Uuid::now_v7(), "quantity": 1}]}),
            "lines[0].product_id",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}, {"product_id": p, "quantity": 1}]}),
            "lines[1].product_id",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "payments": [{"method": "card", "amount": "100"}]}),
            "payments[0].method",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "payments": [{"method": "cash", "amount": "0"}]}),
            "payments[0].amount",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "payments": [{"method": "cash", "amount": "-1"}]}),
            "payments[0].amount",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}],
                "payments": [{"method": "cash", "amount": "50"}, {"method": "cash", "amount": "50"}]}),
            "payments[1].method",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "new_customer": {"phone": "12345"}}),
            "new_customer.phone",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "customer_id": Uuid::now_v7()}),
            "customer_id",
        ),
        (
            json!({"lines": [{"product_id": p, "quantity": 1}], "customer_id": Uuid::now_v7(),
                "new_customer": {"phone": "9862012345"}}),
            "customer_id",
        ),
    ];
    for (body, want) in cases {
        let (got, msg) = field_of(&sh, body.clone()).await;
        assert_eq!(got, want, "{body} -> {msg}");
    }
    let (_, msg) = field_of(&sh, json!({"lines": []})).await;
    assert_eq!(msg, "Add at least one item.");
    assert_eq!(count(&pool, "SELECT count(*) FROM orders").await, 0);
}
