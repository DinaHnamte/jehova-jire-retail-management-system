//! Cancel a delivery and check returned goods back in (Phase 6a Task 6, spec §3.3).
mod common;
use axum::{body::Body, http::Request};
use common::TestResponse;
use common::shop::*;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A delivery sale of `qty` Plates (₹100 each) for `fee`, `paid` in cash up front, plus one
/// Cup when `cup` names one.
async fn sale(sh: &Shop, qty: i64, cup: Option<&str>, fee: &str, paid: &str) -> Value {
    let mut lines = vec![json!({"product_id": sh.p, "quantity": qty})];
    if let Some(c) = cup {
        lines.push(json!({"product_id": c, "quantity": 1}));
    }
    let mut body = json!({
        "lines": lines,
        "new_customer": {"phone": "9862012345", "name": "Mary"},
        "delivery": {
            "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
            "address_line": "House 12, Chanmari West", "locality": "Chanmari",
            "fee": fee, "driver_pay": "40"
        }
    });
    if paid != "0" {
        body["payments"] = json!([{"method": "cash", "amount": paid}]);
    }
    sell(sh, body).await
}

/// A second product, Cup at ₹100, 10 in stock.
async fn cup(sh: &Shop, pool: &PgPool) -> String {
    let cup = common::seed_product(pool, sh.m_id, "Cup").await.to_string();
    let id =
        sh.a.post_json(
            "/api/v1/stock-counts",
            json!({}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await
        .json()["id"]
            .as_str()
            .unwrap()
            .to_string();
    let r =
        sh.a.put_json(
            &format!("/api/v1/stock-counts/{id}/lines"),
            json!({"lines": [{"product_id": cup, "counted": 10, "unit_cost": "60"}]}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let r =
        sh.a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), &sh.m)
            .await;
    assert_eq!(r.status(), 200);
    cup
}

async fn delivery_of(pool: &PgPool, order: &Value) -> String {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM deliveries WHERE order_id = $1")
        .bind(uuid(&order["id"]))
        .fetch_one(pool)
        .await
        .unwrap();
    id.to_string()
}

async fn driver(sh: &Shop) -> String {
    let r =
        sh.a.post_json(
            "/api/v1/drivers",
            json!({"display_name": "Lalbiaka", "username": "lal", "password": "driver password 1",
                   "phone": "9862000001", "default_pay": "40"}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()["id"].as_str().unwrap().to_string()
}

async fn act(sh: &Shop, did: &str, what: &str, body: Value) -> Value {
    let r =
        sh.a.post_json(
            &format!("/api/v1/deliveries/{did}/{what}"),
            body,
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{what}: {:?}", r.json());
    r.json()
}

async fn assign(sh: &Shop, did: &str, driver: &str) {
    act(sh, did, "assign", json!({"driver_id": driver})).await;
}

async fn pick_up(sh: &Shop, did: &str) {
    act(sh, did, "status", json!({"to": "picked_up"})).await;
}

/// Assigned, picked up, then couldn't deliver.
async fn fail(sh: &Shop, did: &str) {
    let lal = driver(sh).await;
    assign(sh, did, &lal).await;
    pick_up(sh, did).await;
    act(
        sh,
        did,
        "status",
        json!({"to": "failed", "reason": "customer_unavailable"}),
    )
    .await;
}

async fn cancel(sh: &Shop, did: &str, key: &str, body: Value) -> TestResponse {
    post(
        &sh.a,
        &sh.m,
        &format!("/api/v1/deliveries/{did}/cancel"),
        key,
        body,
    )
    .await
}

async fn order_cancel(sh: &Shop, order: &Value, key: &str) -> TestResponse {
    post(
        &sh.a,
        &sh.m,
        &format!("/api/v1/orders/{}/cancel", order["id"].as_str().unwrap()),
        key,
        json!({}),
    )
    .await
}

async fn check_in(sh: &Shop, did: &str, key: &str, body: Value) -> TestResponse {
    post(
        &sh.a,
        &sh.m,
        &format!("/api/v1/deliveries/{did}/check-in"),
        key,
        body,
    )
    .await
}

/// Order line ids of `order`, in line order.
async fn line_ids(pool: &PgPool, order: &Value) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT id FROM order_lines WHERE order_id = $1 ORDER BY line_no")
        .bind(uuid(&order["id"]))
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn order_status(pool: &PgPool, order: &Value) -> String {
    sqlx::query_scalar("SELECT status FROM orders WHERE id = $1")
        .bind(uuid(&order["id"]))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn delivery_status(pool: &PgPool, did: &str) -> String {
    sqlx::query_scalar("SELECT status FROM deliveries WHERE id = $1::uuid")
        .bind(did)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn assert_code(r: &TestResponse, status: u16, code: &str) {
    assert_eq!(r.status(), status, "{:?}", r.json());
    assert_eq!(r.json()["error"]["code"], code, "{:?}", r.json());
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

/// Every balance equals the sum of its movements.
async fn stock_is_sound(pool: &PgPool) {
    let bad = count(
        pool,
        "SELECT count(*) FROM stock_balances b
         WHERE b.on_hand <> (SELECT COALESCE(sum(quantity), 0) FROM stock_movements m
                              WHERE m.product_id = b.product_id AND m.location_id = b.location_id)",
    )
    .await;
    assert_eq!(bad, 0);
}

#[sqlx::test]
async fn cancel_before_pickup_restores_stock_and_reverses_fee(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 2, None, "50", "250").await;
    assert_eq!(o["total"], "250.00");
    let did = delivery_of(&pool, &o).await;
    let lal = driver(&sh).await;
    assign(&sh, &did, &lal).await;
    assert_eq!(on_hand(&pool, &sh.p).await, 8);

    // Needs deliveries.cancel, which a cashier doesn't have.
    let r = post(
        &sh.a,
        &sh.c,
        &format!("/api/v1/deliveries/{did}/cancel"),
        &key(),
        json!({}),
    )
    .await;
    assert_eq!(r.status(), 403);
    // And an Idempotency-Key.
    let r =
        sh.a.send(
            Request::post(format!("/api/v1/deliveries/{did}/cancel"))
                .header("content-type", "application/json")
                .header("cookie", &sh.m.cookie)
                .header("x-csrf-token", &sh.m.csrf)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_code(&r, 400, "IDEMPOTENCY_KEY_REQUIRED");

    let k = key();
    let r = cancel(
        &sh,
        &did,
        &k,
        json!({"reason": "Customer changed her mind"}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["id"], did);
    assert_eq!(v["status"], "cancelled");
    assert!(v["closed_at"].is_string());
    assert!(v["driver"].is_null());
    assert_eq!(v["order"]["owed"], "0.00");
    let last = v["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["from"].clone(), last["to"].clone()),
        (json!("assigned"), json!("cancelled"))
    );

    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    assert_eq!(
        account_total(&pool, "delivery_fee_income").await,
        Decimal::ZERO
    );
    assert_eq!(account_total(&pool, "sales_revenue").await, Decimal::ZERO);
    assert_eq!(account_total(&pool, "cash").await, Decimal::ZERO);
    let refund: Decimal = sqlx::query_scalar(
        "SELECT -e.amount FROM ledger_entries e JOIN ledger_transactions t ON t.id = e.transaction_id
         WHERE t.kind = 'refund' AND e.account = 'cash'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refund, d("250"));
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    assert_eq!(order_status(&pool, &o).await, "cancelled");
    let end: String = sqlx::query_scalar("SELECT end_reason FROM delivery_assignments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(end, "cancelled");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM audit_logs WHERE action = 'delivery.cancel'"
        )
        .await,
        1
    );
    ledger_is_sound(&pool).await;
    stock_is_sound(&pool).await;

    // A retry after a lost response gives the same result and writes nothing more.
    let again = cancel(
        &sh,
        &did,
        &k,
        json!({"reason": "Customer changed her mind"}),
    )
    .await;
    assert_eq!(again.status(), 200, "{:?}", again.json());
    assert_eq!(again.json()["status"], "cancelled");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'cancel'"
        )
        .await,
        1
    );
    assert_eq!(on_hand(&pool, &sh.p).await, 10);

    // A new attempt is told it's already cancelled.
    assert_code(
        &cancel(&sh, &did, &key(), json!({})).await,
        409,
        "ALREADY_CANCELLED",
    );
}

#[sqlx::test]
async fn cancel_after_fee_edit_reverses_current_fee(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "50", "0").await;
    let did = delivery_of(&pool, &o).await;
    let r =
        sh.a.send(
            Request::patch(format!("/api/v1/deliveries/{did}"))
                .header("content-type", "application/json")
                .header("cookie", &sh.m.cookie)
                .header("x-csrf-token", &sh.m.csrf)
                .header("idempotency-key", key())
                .body(Body::from(json!({"fee": "80"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(account_total(&pool, "delivery_fee_income").await, d("-80"));
    assert_eq!(order_owed(&pool, &o).await, d("180"));

    // Through the sale's own cancel: the delivery is cancelled with it.
    let r = order_cancel(&sh, &o, &key()).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(
        account_total(&pool, "delivery_fee_income").await,
        Decimal::ZERO
    );
    assert_eq!(account_total(&pool, "sales_revenue").await, Decimal::ZERO);
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    let cancel_entries: Vec<(String, Decimal)> = sqlx::query_as(
        "SELECT e.account, e.amount FROM ledger_entries e
         JOIN ledger_transactions t ON t.id = e.transaction_id
         WHERE t.kind = 'cancel' ORDER BY e.account",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        cancel_entries,
        [
            ("delivery_fee_income".into(), d("80")),
            ("receivable".into(), d("-180")),
            ("sales_revenue".into(), d("100")),
        ]
    );
    assert_eq!(delivery_status(&pool, &did).await, "cancelled");
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn cancel_with_zero_fee_skips_the_fee_entry(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "0", "0").await;
    let did = delivery_of(&pool, &o).await;
    let r = cancel(&sh, &did, &key(), json!({})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let accounts: Vec<String> = sqlx::query_scalar(
        "SELECT e.account FROM ledger_entries e
         JOIN ledger_transactions t ON t.id = e.transaction_id
         WHERE t.kind = 'cancel' ORDER BY e.account",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(accounts, ["receivable", "sales_revenue"]);
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn order_cancel_route_on_picked_up_delivery_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "50", "150").await;
    let did = delivery_of(&pool, &o).await;
    let lal = driver(&sh).await;
    assign(&sh, &did, &lal).await;
    pick_up(&sh, &did).await;

    let r = order_cancel(&sh, &o, &key()).await;
    assert_code(&r, 409, "DELIVERY_OUT");
    assert_eq!(
        r.json()["error"]["message"],
        "This delivery is out with the driver. Mark it delivered or couldn't deliver first."
    );
    assert_code(
        &cancel(&sh, &did, &key(), json!({})).await,
        409,
        "DELIVERY_OUT",
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind IN ('cancel', 'refund')"
        )
        .await,
        0
    );
    assert_eq!(on_hand(&pool, &sh.p).await, 9);
    assert_eq!(order_status(&pool, &o).await, "confirmed");
    assert_eq!(delivery_status(&pool, &did).await, "picked_up");
}

#[sqlx::test]
async fn check_in_restocks_only_sellable(pool: PgPool) {
    let sh = shop(&pool).await;
    let c = cup(&sh, &pool).await;
    let o = sale(&sh, 1, Some(&c), "50", "0").await;
    let did = delivery_of(&pool, &o).await;
    fail(&sh, &did).await;
    assert_eq!(
        (on_hand(&pool, &sh.p).await, on_hand(&pool, &c).await),
        (9, 9)
    );
    let lines = line_ids(&pool, &o).await;

    // Needs deliveries.check_in, which a cashier doesn't have.
    let body = json!({"lines": [
        {"order_line_id": lines[0], "disposition": "sellable"},
        {"order_line_id": lines[1], "disposition": "damaged"}
    ], "trip_pay": "20"});
    let r = post(
        &sh.a,
        &sh.c,
        &format!("/api/v1/deliveries/{did}/check-in"),
        &key(),
        body.clone(),
    )
    .await;
    assert_eq!(r.status(), 403);

    let before =
        sh.a.get_with(&format!("/api/v1/deliveries/{did}"), Some(&sh.m.cookie))
            .await;
    assert_eq!(
        before.json()["items"][1]["order_line_id"],
        lines[1].to_string()
    );

    let r = check_in(&sh, &did, &key(), body).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["status"], "returned");
    assert!(v["closed_at"].is_string());
    assert_eq!(v["attempts"][0]["trip_pay"], "20.00");
    assert_eq!(v["order"]["owed"], "0.00");
    let last = v["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["from"].clone(), last["to"].clone()),
        (json!("failed"), json!("returned"))
    );

    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    assert_eq!(on_hand(&pool, &c).await, 9);
    let cup_moves: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT movement_type, quantity, source_type FROM stock_movements
         WHERE product_id = $1::uuid AND source_type <> 'stock_count'
         ORDER BY created_at, id",
    )
    .bind(&c)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        cup_moves,
        [
            ("sale".into(), -1, "order".into()),
            ("sale_cancel".into(), 1, "order".into()),
            ("damage".into(), -1, "delivery".into()),
        ]
    );
    let damage_source: Uuid =
        sqlx::query_scalar("SELECT source_id FROM stock_movements WHERE movement_type = 'damage'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(damage_source.to_string(), did);

    assert_eq!(order_status(&pool, &o).await, "cancelled");
    let reason: String = sqlx::query_scalar("SELECT cancel_reason FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reason, "Delivery returned");
    assert_eq!(
        account_total(&pool, "delivery_fee_income").await,
        Decimal::ZERO
    );
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM audit_logs WHERE action = 'delivery.check_in'"
        )
        .await,
        1
    );
    ledger_is_sound(&pool).await;
    stock_is_sound(&pool).await;
}

#[sqlx::test]
async fn check_in_must_list_every_line_once(pool: PgPool) {
    let sh = shop(&pool).await;
    let c = cup(&sh, &pool).await;
    let o = sale(&sh, 1, Some(&c), "50", "0").await;
    let did = delivery_of(&pool, &o).await;
    fail(&sh, &did).await;
    let lines = line_ids(&pool, &o).await;
    let msg = "Choose sellable or damaged for every item.";

    let missing = json!({"lines": [{"order_line_id": lines[0], "disposition": "sellable"}]});
    assert_field(&check_in(&sh, &did, &key(), missing).await, "lines", msg);
    let duplicate = json!({"lines": [
        {"order_line_id": lines[0], "disposition": "sellable"},
        {"order_line_id": lines[0], "disposition": "damaged"},
        {"order_line_id": lines[1], "disposition": "damaged"}
    ]});
    assert_field(&check_in(&sh, &did, &key(), duplicate).await, "lines", msg);
    let foreign = json!({"lines": [
        {"order_line_id": lines[0], "disposition": "sellable"},
        {"order_line_id": Uuid::now_v7(), "disposition": "damaged"}
    ]});
    assert_field(&check_in(&sh, &did, &key(), foreign).await, "lines", msg);
    assert_field(
        &check_in(&sh, &did, &key(), json!({"lines": []})).await,
        "lines",
        msg,
    );
    let r = check_in(
        &sh,
        &did,
        &key(),
        json!({"lines": [
            {"order_line_id": lines[0], "disposition": "sellable"},
            {"order_line_id": lines[1], "disposition": "lost"}
        ]}),
    )
    .await;
    assert_eq!(r.status(), 422, "{:?}", r.json());
    assert_eq!(
        r.json()["error"]["details"]["field"],
        "lines[1].disposition"
    );

    // Nothing written.
    assert_eq!(delivery_status(&pool, &did).await, "failed");
    assert_eq!(order_status(&pool, &o).await, "confirmed");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'cancel'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM stock_movements WHERE movement_type IN ('sale_cancel', 'damage')"
        )
        .await,
        0
    );
    assert_eq!(
        (on_hand(&pool, &sh.p).await, on_hand(&pool, &c).await),
        (9, 9)
    );
}

#[sqlx::test]
async fn check_in_retry_with_same_key_once(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 2, None, "50", "250").await;
    let did = delivery_of(&pool, &o).await;
    fail(&sh, &did).await;
    let lines = line_ids(&pool, &o).await;
    let body = json!({"lines": [{"order_line_id": lines[0], "disposition": "damaged"}]});
    let k = key();
    let first = check_in(&sh, &did, &k, body.clone()).await;
    assert_eq!(first.status(), 200, "{:?}", first.json());
    let again = check_in(&sh, &did, &k, body).await;
    assert_eq!(again.status(), 200, "{:?}", again.json());
    assert_eq!(again.json()["status"], "returned");
    // Trip pay left out is ₹0.
    assert_eq!(again.json()["attempts"][0]["trip_pay"], "0.00");

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'cancel'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'refund'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM stock_movements WHERE movement_type = 'damage'"
        )
        .await,
        1
    );
    // 2 back from the cancel, 2 out as damaged.
    assert_eq!(on_hand(&pool, &sh.p).await, 8);
    assert_eq!(account_total(&pool, "cash").await, Decimal::ZERO);
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    ledger_is_sound(&pool).await;
    stock_is_sound(&pool).await;
}

#[sqlx::test]
async fn check_in_only_from_failed(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "50", "0").await;
    let did = delivery_of(&pool, &o).await;
    let lines = line_ids(&pool, &o).await;
    let body = json!({"lines": [{"order_line_id": lines[0], "disposition": "sellable"}]});
    let r = check_in(&sh, &did, &key(), body.clone()).await;
    assert_code(&r, 409, "DELIVERY_STATUS");
    assert_eq!(r.json()["error"]["details"]["status"], "to_send");

    // Cancelled already: still a status conflict, nothing more written.
    assert_eq!(cancel(&sh, &did, &key(), json!({})).await.status(), 200);
    let r = check_in(&sh, &did, &key(), body).await;
    assert_code(&r, 409, "DELIVERY_STATUS");
    assert_eq!(r.json()["error"]["details"]["status"], "cancelled");
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
}

#[sqlx::test]
async fn return_before_delivered_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "50", "150").await;
    let did = delivery_of(&pool, &o).await;
    let lines = line_ids(&pool, &o).await;
    let body = json!({"lines": [{"order_line_id": lines[0], "quantity": 1,
                                 "disposition": "sellable"}],
                      "refund_method": "cash"});
    let give_back = |k: String, b: Value| {
        let sh = &sh;
        let path = format!("/api/v1/orders/{}/returns", o["id"].as_str().unwrap());
        async move { post(&sh.a, &sh.m, &path, &k, b).await }
    };

    let r = give_back(key(), body.clone()).await;
    assert_code(&r, 409, "DELIVERY_NOT_DONE");
    assert_eq!(
        r.json()["error"]["message"],
        "Items can be returned once the delivery is done."
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM returns").await, 0);

    let lal = driver(&sh).await;
    assign(&sh, &did, &lal).await;
    pick_up(&sh, &did).await;
    assert_code(
        &give_back(key(), body.clone()).await,
        409,
        "DELIVERY_NOT_DONE",
    );
    act(
        &sh,
        &did,
        "status",
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await;

    let r = give_back(key(), body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let refunded: Decimal = sqlx::query_scalar("SELECT refunded FROM returns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(refunded, d("100"));
    // Returns never touch the fee.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_entries e JOIN ledger_transactions t
             ON t.id = e.transaction_id
             WHERE e.account = 'delivery_fee_income' AND t.kind <> 'sale'"
        )
        .await,
        0
    );
    assert_eq!(account_total(&pool, "delivery_fee_income").await, d("-50"));
    assert_eq!(on_hand(&pool, &sh.p).await, 10);
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn order_cancel_on_failed_or_delivered_delivery_says_what_to_do(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, None, "50", "150").await;
    let did = delivery_of(&pool, &o).await;
    fail(&sh, &did).await;
    let r = order_cancel(&sh, &o, &key()).await;
    assert_code(&r, 409, "DELIVERY_OUT");
    assert_eq!(
        r.json()["error"]["message"],
        "This delivery couldn't be delivered. Check the goods back in to cancel it."
    );
    assert_eq!(r.json()["error"]["details"]["status"], "failed");

    let o = sale(&sh, 1, None, "50", "150").await;
    let did = delivery_of(&pool, &o).await;
    let lal: String = sqlx::query_scalar("SELECT id::text FROM users WHERE username = 'lal'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assign(&sh, &did, &lal).await;
    pick_up(&sh, &did).await;
    act(
        &sh,
        &did,
        "status",
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await;
    let r = order_cancel(&sh, &o, &key()).await;
    assert_code(&r, 409, "DELIVERY_OUT");
    assert_eq!(
        r.json()["error"]["message"],
        "This delivery is done. Return items instead."
    );
    assert_eq!(r.json()["error"]["details"]["status"], "delivered");
}

/// A free Plate delivered for free: no sale transaction is written at checkout.
async fn free_sale(sh: &Shop) -> Value {
    sell(
        sh,
        json!({
            "lines": [{"product_id": sh.p, "quantity": 1, "unit_price": "0"}],
            "new_customer": {"phone": "9862012345", "name": "Mary"},
            "delivery": {
                "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
                "address_line": "House 12, Chanmari West", "locality": "Chanmari",
                "fee": "0"
            }
        }),
    )
    .await
}

#[sqlx::test]
async fn cancel_free_order_after_fee_raised(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = free_sale(&sh).await;
    let did = delivery_of(&pool, &o).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'sale'"
        )
        .await,
        0
    );
    set_fee(&sh, &did, "30").await;
    assert_eq!(order_owed(&pool, &o).await, Decimal::from(30));
    let r = cancel(&sh, &did, &key(), json!({})).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(delivery_status(&pool, &did).await, "cancelled");
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    assert_eq!(
        account_total(&pool, "delivery_fee_income").await,
        Decimal::ZERO
    );
    ledger_is_sound(&pool).await;
    stock_is_sound(&pool).await;
}

#[sqlx::test]
async fn check_in_free_order_after_fee_raised(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = free_sale(&sh).await;
    let did = delivery_of(&pool, &o).await;
    set_fee(&sh, &did, "30").await;
    fail(&sh, &did).await;
    let lines = line_ids(&pool, &o).await;
    let r = check_in(
        &sh,
        &did,
        &key(),
        json!({"lines": [{"order_line_id": lines[0], "disposition": "sellable"}]}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(r.json()["status"], "returned");
    assert_eq!(order_owed(&pool, &o).await, Decimal::ZERO);
    assert_eq!(
        account_total(&pool, "delivery_fee_income").await,
        Decimal::ZERO
    );
    ledger_is_sound(&pool).await;
    stock_is_sound(&pool).await;
}
