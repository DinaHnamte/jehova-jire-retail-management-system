//! Stock check and the Phase 4 exit criterion: every unit on hand traces to a receipt or count.
mod common;
use common::{Session, TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn boss(pool: &PgPool) -> (TestApp, Session) {
    seed_user(pool, "boss", PW, &["admin"]).await;
    let a = app(pool.clone());
    let s = a.login("boss", PW).await;
    (a, s)
}

async fn count(a: &TestApp, s: &Session, lines: Value) {
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
    assert_eq!(
        a.put_json(
            &format!("/api/v1/stock-counts/{id}/lines"),
            json!({"lines": lines}),
            Some(&s.cookie),
            Some(&s.csrf)
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), s)
            .await
            .status(),
        200
    );
}

async fn receipt(
    a: &TestApp,
    s: &Session,
    supplier: &Value,
    lines: Value,
    confirm: bool,
) -> String {
    let id = a
        .post_json(
            "/api/v1/goods-receipts",
            json!({"supplier_id": supplier}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        a.put_json(
            &format!("/api/v1/goods-receipts/{id}/lines"),
            json!({"lines": lines}),
            Some(&s.cookie),
            Some(&s.csrf)
        )
        .await
        .status(),
        200
    );
    if confirm {
        assert_eq!(
            a.post_empty(&format!("/api/v1/goods-receipts/{id}/confirm"), s)
                .await
                .status(),
            200
        );
    }
    id
}

async fn run_check(a: &TestApp, s: &Session) -> Value {
    let r = a.post_empty("/api/v1/stock-checks", s).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

#[sqlx::test]
async fn clean_ledger_has_no_mismatches(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    count(&a, &s, json!([{"product_id": p, "counted": 4}])).await;
    assert_eq!(run_check(&a, &s).await["mismatches"], json!([]));
}

#[sqlx::test]
async fn planted_mismatch_is_reported_not_fixed(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    count(&a, &s, json!([{"product_id": p, "counted": 4}])).await;
    sqlx::query("UPDATE stock_balances SET on_hand = on_hand + 1")
        .execute(&pool)
        .await
        .unwrap();
    let v = run_check(&a, &s).await;
    assert_eq!(v["mismatches"].as_array().unwrap().len(), 1);
    assert_eq!(v["mismatches"][0]["kind"], "stock");
    assert_eq!(v["mismatches"][0]["product_id"], json!(p));
    assert_eq!(v["mismatches"][0]["code"], "B6143-8");
    assert_eq!(v["mismatches"][0]["name"], "Bowl");
    assert_eq!(v["mismatches"][0]["balance"], 5);
    assert_eq!(v["mismatches"][0]["ledger_sum"], 4);
    let on_hand: i64 = sqlx::query_scalar("SELECT on_hand FROM stock_balances")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(on_hand, 5, "the check never corrects");
}

#[sqlx::test]
async fn latest_returns_most_recent_run(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    assert_eq!(
        a.get_with("/api/v1/stock-checks/latest", Some(&s.cookie))
            .await
            .json(),
        Value::Null
    );
    run_check(&a, &s).await;
    let second = run_check(&a, &s).await;
    let v = a
        .get_with("/api/v1/stock-checks/latest", Some(&s.cookie))
        .await
        .json();
    assert_eq!(v["id"], second["id"]);
}

#[sqlx::test]
async fn cashier_cannot_run_check(pool: PgPool) {
    let (a, _) = boss(&pool).await;
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let c = a.login("cash", PW).await;
    assert_eq!(a.post_empty("/api/v1/stock-checks", &c).await.status(), 403);
    assert_eq!(
        a.get_with("/api/v1/stock-checks/latest", Some(&c.cookie))
            .await
            .status(),
        200
    );
}

/// Phase 4 exit criterion (Architecture.md §20).
#[sqlx::test]
async fn every_unit_traces_to_a_confirmed_source(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p1 = a.product(&s, "B 6143-8", "Bowl").await;
    let p2 = a.product(&s, "IP 1060-11", "Plate").await;
    let p3 = a.product(&s, "MP4952", "Pink plate").await;
    let sup = a
        .post_json(
            "/api/v1/suppliers",
            json!({"name": "Superware"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await
        .json()["id"]
        .clone();

    count(
        &a,
        &s,
        json!([
            {"product_id": p1, "counted": 10, "unit_cost": "100"},
            {"product_id": p2, "counted": 4},
            {"product_id": p3, "counted": 0}
        ]),
    )
    .await;
    receipt(&a, &s, &sup, json!([{"product_id": p1, "quantity": 5, "unit_cost": "130"}, {"product_id": p3, "quantity": 6, "unit_cost": "50"}]), true).await;
    receipt(
        &a,
        &s,
        &sup,
        json!([{"product_id": p2, "quantity": 2, "unit_cost": "75"}]),
        true,
    )
    .await;
    let r = a
        .post_json(
            &format!("/api/v1/products/{p1}/adjustments"),
            json!({"type": "damage", "quantity": 1, "reason": "Cracked"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 201);
    count(&a, &s, json!([{"product_id": p2, "counted": 5}])).await;
    let draft = receipt(
        &a,
        &s,
        &sup,
        json!([{"product_id": p1, "quantity": 99, "unit_cost": "1"}]),
        false,
    )
    .await;

    let bad: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_balances b
         WHERE b.on_hand <> (SELECT COALESCE(sum(quantity), 0) FROM stock_movements m
                              WHERE m.product_id = b.product_id AND m.location_id = b.location_id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bad, 0, "every balance equals its ledger sum");

    let untraced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements m
         WHERE NOT (
            (m.source_type = 'goods_receipt' AND EXISTS (
                SELECT 1 FROM goods_receipts r WHERE r.id = m.source_id AND r.status = 'confirmed'))
         OR (m.source_type = 'stock_count' AND EXISTS (
                SELECT 1 FROM stock_counts c WHERE c.id = m.source_id AND c.status = 'posted'))
         OR (m.source_type = 'adjustment' AND EXISTS (
                SELECT 1 FROM audit_logs a WHERE a.action = 'stock.adjust' AND a.entity_id = m.source_id)))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(untraced, 0, "every movement traces to a final document");

    let from_draft: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stock_movements WHERE source_id = $1::uuid")
            .bind(&draft)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(from_draft, 0);

    let on_hand: Vec<(String, i64)> = sqlx::query_as(
        "SELECT p.name, b.on_hand FROM stock_balances b JOIN products p ON p.id = b.product_id ORDER BY p.name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        on_hand,
        [
            ("Bowl".into(), 14),
            ("Pink plate".into(), 6),
            ("Plate".into(), 5)
        ]
    );
    assert_eq!(run_check(&a, &s).await["mismatches"], json!([]));
}

#[sqlx::test]
async fn stock_check_flags_receivable_out_of_range(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    count(&a, &s, json!([{"product_id": p, "counted": 4}])).await;
    let r = a
        .send(
            axum::http::Request::post("/api/v1/orders/checkout")
                .header("content-type", "application/json")
                .header("cookie", &s.cookie)
                .header("x-csrf-token", &s.csrf)
                .header("idempotency-key", "key-over-0001")
                .body(axum::body::Body::from(
                    json!({"lines": [{"product_id": p, "quantity": 1}], "payments": [{"method": "cash", "amount": "150"}]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let order_id = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(run_check(&a, &s).await["mismatches"], json!([]));

    // A balanced transaction that over-credits the paid order: owed becomes -10.
    sqlx::query(
        "WITH t AS (INSERT INTO ledger_transactions (id, kind, order_id, method, actor_id, idempotency_key)
                    SELECT gen_random_uuid(), 'payment', $1::uuid, 'cash', created_by, 'plant-over-1'
                    FROM orders WHERE id = $1::uuid RETURNING id)
         INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
         SELECT gen_random_uuid(), t.id, 'cash', NULL, 10 FROM t
         UNION ALL SELECT gen_random_uuid(), t.id, 'receivable', $1::uuid, -10 FROM t",
    )
    .bind(&order_id)
    .execute(&pool)
    .await
    .unwrap();
    let v = run_check(&a, &s).await;
    assert_eq!(
        v["mismatches"],
        json!([{"kind": "receivable_out_of_range", "order_id": order_id, "owed": "-10.00"}])
    );
}

#[sqlx::test]
async fn stock_check_flags_cancelled_order_with_owed(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    count(&a, &s, json!([{"product_id": p, "counted": 4}])).await;
    let post = |path: String, key: &'static str, body: Value| {
        let (a, s) = (&a, &s);
        async move {
            a.send(
                axum::http::Request::post(path)
                    .header("content-type", "application/json")
                    .header("cookie", &s.cookie)
                    .header("x-csrf-token", &s.csrf)
                    .header("idempotency-key", key)
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
        }
    };
    let r = post(
        "/api/v1/orders/checkout".into(),
        "key-canc-0001",
        json!({"lines": [{"product_id": p, "quantity": 1}], "payments": [{"method": "cash", "amount": "150"}]}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let order_id = r.json()["id"].as_str().unwrap().to_string();
    let c = post(
        format!("/api/v1/orders/{order_id}/cancel"),
        "key-canc-0002",
        json!({}),
    )
    .await;
    assert_eq!(c.status(), 200, "{:?}", c.json());
    assert_eq!(run_check(&a, &s).await["mismatches"], json!([]));

    // A balanced transaction that leaves the cancelled order owing 10.
    sqlx::query(
        "WITH t AS (INSERT INTO ledger_transactions (id, kind, order_id, method, actor_id, idempotency_key)
                    SELECT gen_random_uuid(), 'payment', $1::uuid, 'cash', created_by, 'plant-canc-1'
                    FROM orders WHERE id = $1::uuid RETURNING id)
         INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
         SELECT gen_random_uuid(), t.id, 'cash', NULL, -10 FROM t
         UNION ALL SELECT gen_random_uuid(), t.id, 'receivable', $1::uuid, 10 FROM t",
    )
    .bind(&order_id)
    .execute(&pool)
    .await
    .unwrap();
    let v = run_check(&a, &s).await;
    assert_eq!(
        v["mismatches"],
        json!([{"kind": "receivable_out_of_range", "order_id": order_id, "owed": "10.00"}])
    );
}

#[sqlx::test]
async fn stock_check_bounds_owed_by_total_less_returns(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    count(&a, &s, json!([{"product_id": p, "counted": 4}])).await;
    let post = |path: String, key: &'static str, body: Value| {
        let (a, s) = (&a, &s);
        async move {
            a.send(
                axum::http::Request::post(path)
                    .header("content-type", "application/json")
                    .header("cookie", &s.cookie)
                    .header("x-csrf-token", &s.csrf)
                    .header("idempotency-key", key)
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
        }
    };
    // Two Bowls at 150 on credit: owed 300.
    let r = post(
        "/api/v1/orders/checkout".into(),
        "key-ret-0001",
        json!({"lines": [{"product_id": p, "quantity": 2}], "new_customer": {"phone": "9862033333"}}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let order_id = r.json()["id"].as_str().unwrap().to_string();
    let line_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM order_lines WHERE order_id = $1::uuid")
            .bind(&order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    // One back: owed 150, and at most 150 can still be owed.
    let ret = post(
        format!("/api/v1/orders/{order_id}/returns"),
        "key-ret-0002",
        json!({"lines": [{"order_line_id": line_id, "quantity": 1, "disposition": "sellable"}]}),
    )
    .await;
    assert_eq!(ret.status(), 201, "{:?}", ret.json());
    assert_eq!(run_check(&a, &s).await["mismatches"], json!([]));

    // A balanced transaction that adds 10 to what is owed: 160 is under the total 300 but over 150.
    sqlx::query(
        "WITH t AS (INSERT INTO ledger_transactions (id, kind, order_id, method, actor_id, idempotency_key)
                    SELECT gen_random_uuid(), 'payment', $1::uuid, 'cash', created_by, 'plant-ret-1'
                    FROM orders WHERE id = $1::uuid RETURNING id)
         INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
         SELECT gen_random_uuid(), t.id, 'cash', NULL, -10 FROM t
         UNION ALL SELECT gen_random_uuid(), t.id, 'receivable', $1::uuid, 10 FROM t",
    )
    .bind(&order_id)
    .execute(&pool)
    .await
    .unwrap();
    let v = run_check(&a, &s).await;
    assert_eq!(
        v["mismatches"],
        json!([{"kind": "receivable_out_of_range", "order_id": order_id, "owed": "160.00"}])
    );
}

mod delivery_fee {
    use super::common::shop::*;
    use serde_json::json;
    use sqlx::PgPool;

    #[sqlx::test]
    async fn stock_check_accepts_fee_increase(pool: PgPool) {
        let sh = shop(&pool).await;
        let o = delivery_sale(&sh, "50", "0").await;
        let did = delivery_id(&pool, &o).await;
        set_fee(&sh, &did, "80").await;
        assert_eq!(order_owed(&pool, &o).await, d("180"));
        let r = sh.a.post_empty("/api/v1/stock-checks", &sh.m).await;
        assert_eq!(r.status(), 201, "{:?}", r.json());
        assert_eq!(r.json()["mismatches"], json!([]));
    }
}
