//! Goods receipts (spec §2 rules 2, 3, 6; plan Task 5).
mod common;
use common::{Session, TestApp, app, seed_user};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

const PW: &str = "correct horse battery";

struct Ctx {
    a: TestApp,
    s: Session,
    pool: PgPool,
    supplier: String,
}

async fn ctx(pool: PgPool) -> Ctx {
    seed_user(&pool, "boss", PW, &["manager"]).await;
    let a = app(pool.clone());
    let s = a.login("boss", PW).await;
    let r = a
        .post_json(
            "/api/v1/suppliers",
            json!({"name": "Superware"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    let supplier = r.json()["id"].as_str().unwrap().to_string();
    Ctx {
        a,
        s,
        pool,
        supplier,
    }
}

impl Ctx {
    async fn receipt(&self) -> String {
        let r = self
            .a
            .post_json(
                "/api/v1/goods-receipts",
                json!({"supplier_id": self.supplier}),
                Some(&self.s.cookie),
                Some(&self.s.csrf),
            )
            .await;
        assert_eq!(r.status(), 201, "{:?}", r.json());
        assert_eq!(r.json()["status"], "draft");
        r.json()["id"].as_str().unwrap().to_string()
    }
    async fn lines(&self, id: &str, lines: Value) -> common::TestResponse {
        self.a
            .put_json(
                &format!("/api/v1/goods-receipts/{id}/lines"),
                json!({"lines": lines}),
                Some(&self.s.cookie),
                Some(&self.s.csrf),
            )
            .await
    }
    async fn confirm(&self, id: &str) -> common::TestResponse {
        self.a
            .post_empty(&format!("/api/v1/goods-receipts/{id}/confirm"), &self.s)
            .await
    }
    async fn balance(&self, product: &str) -> Option<(i64, Option<Decimal>)> {
        sqlx::query_as("SELECT on_hand, avg_cost FROM stock_balances WHERE product_id = $1")
            .bind(Uuid::parse_str(product).unwrap())
            .fetch_optional(&self.pool)
            .await
            .unwrap()
    }
    async fn movements(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM stock_movements")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[sqlx::test]
async fn draft_lifecycle(pool: PgPool) {
    let c = ctx(pool).await;
    let p1 = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let p2 = c.a.product(&c.s, "MP4952", "Plate").await;
    let id = c.receipt().await;
    let r = c
        .lines(
            &id,
            json!([
                {"product_id": p1, "quantity": 10, "unit_cost": "120.50"},
                {"product_id": p2, "quantity": 3, "unit_cost": "80"}
            ]),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v =
        c.a.get_with(&format!("/api/v1/goods-receipts/{id}"), Some(&c.s.cookie))
            .await
            .json();
    assert_eq!(v["status"], "draft");
    assert_eq!(v["total"], "1445.00");
    assert_eq!(v["supplier"]["name"], "Superware");
    assert_eq!(v["lines"][0]["code"], "B 6143-8");
    assert_eq!(v["lines"][0]["line_total"], "1205.00");
    let list =
        c.a.get_with("/api/v1/goods-receipts", Some(&c.s.cookie))
            .await
            .json();
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["line_count"], 2);
    assert_eq!(c.balance(&p1).await, None, "a draft adds no stock");
}

#[sqlx::test]
async fn confirm_adds_stock_and_sets_avg_cost(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 10, "unit_cost": "120.50"}]),
    )
    .await;
    let r = c.confirm(&id).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    assert_eq!(r.json()["status"], "confirmed");
    assert_eq!(r.json()["confirmed_by_name"], "boss");
    assert_eq!(c.balance(&p).await, Some((10, Some(d("120.5")))));
}

#[sqlx::test]
async fn second_receipt_blends_avg_cost(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    for cost in ["100", "120"] {
        let id = c.receipt().await;
        c.lines(
            &id,
            json!([{"product_id": p, "quantity": 10, "unit_cost": cost}]),
        )
        .await;
        assert_eq!(c.confirm(&id).await.status(), 200);
    }
    assert_eq!(c.balance(&p).await, Some((20, Some(d("110")))));
}

#[sqlx::test]
async fn confirm_twice_is_409_and_writes_nothing(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 10, "unit_cost": "100"}]),
    )
    .await;
    c.confirm(&id).await;
    let r = c.confirm(&id).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "ALREADY_CONFIRMED");
    assert_eq!(c.movements().await, 1);
}

#[sqlx::test]
async fn concurrent_confirms_one_wins(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 10, "unit_cost": "100"}]),
    )
    .await;
    let (x, y) = tokio::join!(c.confirm(&id), c.confirm(&id));
    let mut st = [x.status(), y.status()];
    st.sort();
    assert_eq!(st, [200, 409]);
    assert_eq!(c.balance(&p).await.unwrap().0, 10);
}

#[sqlx::test]
async fn confirmed_receipt_cannot_be_edited_or_deleted(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 10, "unit_cost": "100"}]),
    )
    .await;
    c.confirm(&id).await;
    let path = format!("/api/v1/goods-receipts/{id}");
    let r =
        c.a.patch_json(
            &path,
            json!({"note": "x"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await;
    assert_eq!(
        (r.status(), r.json()["error"]["code"].clone()),
        (409, json!("NOT_DRAFT"))
    );
    let r = c
        .lines(
            &id,
            json!([{"product_id": p, "quantity": 1, "unit_cost": "1"}]),
        )
        .await;
    assert_eq!(
        (r.status(), r.json()["error"]["code"].clone()),
        (409, json!("NOT_DRAFT"))
    );
    let r =
        c.a.delete_with(&path, Some(&c.s.cookie), Some(&c.s.csrf))
            .await;
    assert_eq!(
        (r.status(), r.json()["error"]["code"].clone()),
        (409, json!("NOT_DRAFT"))
    );
    assert_eq!(c.balance(&p).await.unwrap().0, 10);
}

#[sqlx::test]
async fn draft_can_be_edited_and_deleted(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.receipt().await;
    let path = format!("/api/v1/goods-receipts/{id}");
    let r =
        c.a.patch_json(
            &path,
            json!({"note": "Bill 4411", "received_on": "2026-11-01"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["note"], "Bill 4411");
    assert_eq!(r.json()["received_on"], "2026-11-01");
    assert_eq!(
        c.a.delete_with(&path, Some(&c.s.cookie), Some(&c.s.csrf))
            .await
            .status(),
        204
    );
    assert_eq!(c.a.get_with(&path, Some(&c.s.cookie)).await.status(), 404);
}

#[sqlx::test]
async fn bad_line_numbers_are_field_errors(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let q = c.a.product(&c.s, "B 6143-10", "Bowl big").await;
    let id = c.receipt().await;
    let cases = [
        (
            json!([{"product_id": p, "quantity": 0, "unit_cost": "1"}]),
            "lines[0].quantity",
        ),
        (
            json!([{"product_id": p, "quantity": -2, "unit_cost": "1"}]),
            "lines[0].quantity",
        ),
        (
            json!([{"product_id": p, "quantity": 1, "unit_cost": "12.345"}]),
            "lines[0].unit_cost",
        ),
        (
            json!([{"product_id": p, "quantity": 1, "unit_cost": "abc"}]),
            "lines[0].unit_cost",
        ),
        (
            json!([{"product_id": q, "quantity": 1, "unit_cost": "1"}, {"product_id": q, "quantity": 2, "unit_cost": "1"}]),
            "lines[1].product_id",
        ),
        (
            json!([{"product_id": Uuid::now_v7(), "quantity": 1, "unit_cost": "1"}]),
            "lines[0].product_id",
        ),
    ];
    for (lines, field) in cases {
        let r = c.lines(&id, lines.clone()).await;
        assert_eq!(r.status(), 422, "{lines}");
        assert_eq!(r.json()["error"]["details"]["field"], field, "{lines}");
    }
    let v =
        c.a.get_with(&format!("/api/v1/goods-receipts/{id}"), Some(&c.s.cookie))
            .await
            .json();
    assert_eq!(
        v["lines"].as_array().unwrap().len(),
        0,
        "a failed save keeps the old lines"
    );
}

#[sqlx::test]
async fn archived_product_blocks_confirm(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 5, "unit_cost": "10"}]),
    )
    .await;
    assert_eq!(
        c.a.post_empty(&format!("/api/v1/products/{p}/archive"), &c.s)
            .await
            .status(),
        200
    );
    let r = c.confirm(&id).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "lines[0].product_id");
    assert_eq!(
        r.json()["error"]["message"],
        "B 6143-8 is archived. Remove it from this receipt."
    );
    assert_eq!(c.movements().await, 0);
}

#[sqlx::test]
async fn archived_supplier_blocks_confirm(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 5, "unit_cost": "10"}]),
    )
    .await;
    c.a.patch_json(
        &format!("/api/v1/suppliers/{}", c.supplier),
        json!({"archived": true}),
        Some(&c.s.cookie),
        Some(&c.s.csrf),
    )
    .await;
    let r = c.confirm(&id).await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json()["error"]["message"],
        "Superware is archived. Pick another supplier."
    );
}

#[sqlx::test]
async fn confirm_without_lines_is_422(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.receipt().await;
    let r = c.confirm(&id).await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json()["error"]["message"],
        "Add at least one product before confirming."
    );
}

#[sqlx::test]
async fn cashier_cannot_create_receipt(pool: PgPool) {
    let c = ctx(pool.clone()).await;
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let s = c.a.login("cash", PW).await;
    let r =
        c.a.post_json(
            "/api/v1/goods-receipts",
            json!({"supplier_id": c.supplier}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 403);
}

#[sqlx::test]
async fn confirm_is_audited(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.receipt().await;
    c.lines(
        &id,
        json!([{"product_id": p, "quantity": 2, "unit_cost": "10"}]),
    )
    .await;
    c.confirm(&id).await;
    let after: Value = sqlx::query_scalar(
        "SELECT after FROM audit_logs WHERE action = 'goods_receipt.confirm' AND entity_id = $1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(after["total"], "20.00");
}
