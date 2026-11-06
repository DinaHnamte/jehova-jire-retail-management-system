//! Daily sales summary (Phase 5a Task 9).
mod common;
use axum::{body::Body, http::Request};
use common::{Session, TestApp, TestResponse, app, seed_product, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Shop {
    a: TestApp,
    boss: Session,
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
    a.put_json(
        &format!("/api/v1/stock-counts/{id}/lines"),
        json!({"lines": [{"product_id": p, "counted": 50, "unit_cost": "60"}]}),
        Some(&boss.cookie),
        Some(&boss.csrf),
    )
    .await;
    a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), &boss)
        .await;
    let cashier = a.login("cashier", PW).await;
    Shop {
        a,
        boss,
        cashier,
        p,
    }
}

async fn post(sh: &Shop, path: &str, body: Value) -> TestResponse {
    sh.a.send(
        Request::post(path)
            .header("content-type", "application/json")
            .header("cookie", &sh.cashier.cookie)
            .header("x-csrf-token", &sh.cashier.csrf)
            .header("idempotency-key", format!("k-{}", Uuid::now_v7()))
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// Sells `qty` Plates (100 each) to Mary, creating her on the first sale.
async fn sell(sh: &Shop, qty: i64, payments: Value) -> Value {
    let mut body = json!({"lines": [{"product_id": sh.p, "quantity": qty}], "payments": payments});
    let cid: Option<String> = sqlx::query_scalar("SELECT id::text FROM customers")
        .fetch_optional(&sh.a.pool)
        .await
        .unwrap();
    match cid {
        Some(id) => body["customer_id"] = json!(id),
        None => body["new_customer"] = json!({"phone": "9862012345", "name": "Mary"}),
    }
    let r = post(sh, "/api/v1/orders/checkout", body).await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()
}

async fn daily(sh: &Shop, s: &Session, query: &str) -> TestResponse {
    sh.a.get_with(&format!("/api/v1/reports/daily{query}"), Some(&s.cookie))
        .await
}

#[sqlx::test]
async fn daily_summary_reconciles(pool: PgPool) {
    let sh = shop(&pool).await;
    // Yesterday: a pay-later sale of 100, backdated (the ledger refuses updates, so lift its guard).
    let old = sell(&sh, 1, json!([])).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE ledger_transactions DISABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET created_at = now() - interval '1 day' WHERE id = $1::uuid")
        .bind(old["id"].as_str().unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE ledger_transactions SET occurred_at = now() - interval '1 day' WHERE order_id = $1::uuid",
    )
    .bind(old["id"].as_str().unwrap())
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE ledger_transactions ENABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    sell(&sh, 2, json!([{"method": "cash", "amount": "200"}])).await;
    sell(&sh, 3, json!([{"method": "upi", "amount": "150"}])).await;
    sell(&sh, 1, json!([])).await;
    let cid: String = sqlx::query_scalar("SELECT id::text FROM customers")
        .fetch_one(&pool)
        .await
        .unwrap();
    let r = post(
        &sh,
        &format!("/api/v1/customers/{cid}/payments"),
        json!({"method": "cash", "amount": "50"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());

    let r = daily(&sh, &sh.boss, "").await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert_eq!(v["sales_count"], 3);
    assert_eq!(v["sales_total"], "600.00");
    assert_eq!(v["discounts"], "0.00");
    assert_eq!(v["cash_received"], "250.00");
    assert_eq!(v["upi_received"], "150.00");
    assert_eq!(v["new_owed"], "250.00");
    assert_eq!(v["total_owed"], "300.00");
    // The card adds up by eye: today's sales were paid for or left owing, and the money
    // taken today went to today's sales or to older ones.
    assert_eq!(v["paid_on_these_sales"], "350.00");
    assert_eq!(v["received_on_older_sales"], "50.00");
    // 600 = 350 + 250, and 250 + 150 = 350 + 50.
}

#[sqlx::test]
async fn daily_summary_other_date_is_empty(pool: PgPool) {
    let sh = shop(&pool).await;
    sell(&sh, 1, json!([{"method": "cash", "amount": "100"}])).await;
    let v = daily(&sh, &sh.boss, "?date=2020-01-01").await.json();
    assert_eq!(v["date"], "2020-01-01");
    assert_eq!(v["sales_count"], 0);
    for k in [
        "sales_total",
        "discounts",
        "cash_received",
        "upi_received",
        "new_owed",
        "total_owed",
        "paid_on_these_sales",
        "received_on_older_sales",
    ] {
        assert_eq!(v[k], "0.00", "{k}");
    }
}

#[sqlx::test]
async fn daily_summary_rejects_bad_date(pool: PgPool) {
    let sh = shop(&pool).await;
    let r = daily(&sh, &sh.boss, "?date=yesterday").await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "date");
}

#[sqlx::test]
async fn daily_summary_needs_reports_read(pool: PgPool) {
    let sh = shop(&pool).await;
    assert_eq!(daily(&sh, &sh.cashier, "").await.status(), 403);
}
