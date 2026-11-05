//! Stock counts (spec §2 rule 4; plan Task 6). The first count of a product is its opening balance.
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
}

async fn ctx(pool: PgPool) -> Ctx {
    seed_user(&pool, "boss", PW, &["manager"]).await;
    let a = app(pool.clone());
    let s = a.login("boss", PW).await;
    Ctx { a, s, pool }
}

impl Ctx {
    async fn count(&self) -> String {
        let r = self
            .a
            .post_json(
                "/api/v1/stock-counts",
                json!({"note": "Shelf A"}),
                Some(&self.s.cookie),
                Some(&self.s.csrf),
            )
            .await;
        assert_eq!(r.status(), 201, "{:?}", r.json());
        r.json()["id"].as_str().unwrap().to_string()
    }
    async fn lines(&self, id: &str, lines: Value) -> common::TestResponse {
        self.a
            .put_json(
                &format!("/api/v1/stock-counts/{id}/lines"),
                json!({"lines": lines}),
                Some(&self.s.cookie),
                Some(&self.s.csrf),
            )
            .await
    }
    async fn post(&self, id: &str) -> common::TestResponse {
        self.a
            .post_empty(&format!("/api/v1/stock-counts/{id}/post"), &self.s)
            .await
    }
    /// Create, fill and post a count in one go.
    async fn posted(&self, lines: Value) -> Value {
        let id = self.count().await;
        let r = self.lines(&id, lines).await;
        assert_eq!(r.status(), 200, "{:?}", r.json());
        let r = self.post(&id).await;
        assert_eq!(r.status(), 200, "{:?}", r.json());
        r.json()
    }
    async fn balance(&self, p: &str) -> Option<(i64, Option<Decimal>)> {
        sqlx::query_as("SELECT on_hand, avg_cost FROM stock_balances WHERE product_id = $1")
            .bind(Uuid::parse_str(p).unwrap())
            .fetch_optional(&self.pool)
            .await
            .unwrap()
    }
    async fn moves(&self, p: &str) -> Vec<(String, i64, Option<String>)> {
        sqlx::query_as("SELECT movement_type, quantity, reason FROM stock_movements WHERE product_id = $1 ORDER BY created_at, id")
            .bind(Uuid::parse_str(p).unwrap())
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[sqlx::test]
async fn first_count_is_opening_balance_with_cost(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let v = c
        .posted(json!([{"product_id": p, "counted": 12, "unit_cost": "95"}]))
        .await;
    assert_eq!(v["status"], "posted");
    assert_eq!(v["lines"][0]["expected"], 0);
    assert_eq!(
        c.moves(&p).await,
        [("opening_balance".to_string(), 12, None)]
    );
    assert_eq!(c.balance(&p).await, Some((12, Some(d("95")))));
}

#[sqlx::test]
async fn first_count_without_cost_leaves_cost_unknown(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    c.posted(json!([{"product_id": p, "counted": 4}])).await;
    assert_eq!(c.balance(&p).await, Some((4, None)));
}

#[sqlx::test]
async fn recount_writes_adjustments(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    c.posted(json!([{"product_id": p, "counted": 12, "unit_cost": "95"}]))
        .await;
    let v = c.posted(json!([{"product_id": p, "counted": 10}])).await;
    assert_eq!(v["lines"][0]["expected"], 12);
    assert_eq!(v["lines"][0]["difference"], -2);
    assert_eq!(
        c.moves(&p).await[1],
        (
            "adjustment_out".to_string(),
            -2,
            Some("Stock count".to_string())
        )
    );
    assert_eq!(c.balance(&p).await, Some((10, Some(d("95")))));
}

#[sqlx::test]
async fn recount_up_writes_adjustment_in_and_keeps_avg(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    c.posted(json!([{"product_id": p, "counted": 12, "unit_cost": "95"}]))
        .await;
    c.posted(json!([{"product_id": p, "counted": 15, "unit_cost": "500"}]))
        .await;
    assert_eq!(
        c.moves(&p).await[1],
        (
            "adjustment_in".to_string(),
            3,
            Some("Stock count".to_string())
        )
    );
    assert_eq!(
        c.balance(&p).await,
        Some((15, Some(d("95")))),
        "cost on a recount is ignored"
    );
}

#[sqlx::test]
async fn unchanged_line_writes_no_movement(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    c.posted(json!([{"product_id": p, "counted": 12}])).await;
    let v = c.posted(json!([{"product_id": p, "counted": 12}])).await;
    assert_eq!(v["lines"][0]["difference"], 0);
    assert_eq!(c.moves(&p).await.len(), 1);
}

#[sqlx::test]
async fn first_count_of_zero_writes_nothing_and_next_count_is_opening(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let v = c.posted(json!([{"product_id": p, "counted": 0}])).await;
    assert_eq!(v["status"], "posted");
    assert!(c.moves(&p).await.is_empty());
    c.posted(json!([{"product_id": p, "counted": 6, "unit_cost": "40"}]))
        .await;
    assert_eq!(
        c.moves(&p).await,
        [("opening_balance".to_string(), 6, None)]
    );
    assert_eq!(c.balance(&p).await, Some((6, Some(d("40")))));
}

#[sqlx::test]
async fn changed_since_start_flags_receipt_during_count(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let q = c.a.product(&c.s, "B 6143-10", "Bowl big").await;
    let id = c.count().await;
    c.lines(
        &id,
        json!([{"product_id": p, "counted": 3}, {"product_id": q, "counted": 1}]),
    )
    .await;
    let sup =
        c.a.post_json(
            "/api/v1/suppliers",
            json!({"name": "S"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await
        .json()["id"]
            .clone();
    let rid =
        c.a.post_json(
            "/api/v1/goods-receipts",
            json!({"supplier_id": sup}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await
        .json()["id"]
            .as_str()
            .unwrap()
            .to_string();
    c.a.put_json(
        &format!("/api/v1/goods-receipts/{rid}/lines"),
        json!({"lines": [{"product_id": p, "quantity": 5, "unit_cost": "10"}]}),
        Some(&c.s.cookie),
        Some(&c.s.csrf),
    )
    .await;
    assert_eq!(
        c.a.post_empty(&format!("/api/v1/goods-receipts/{rid}/confirm"), &c.s)
            .await
            .status(),
        200
    );
    let v =
        c.a.get_with(&format!("/api/v1/stock-counts/{id}"), Some(&c.s.cookie))
            .await
            .json();
    assert_eq!(v["lines"][0]["changed_since_start"], true);
    assert_eq!(v["lines"][0]["on_hand_now"], 5);
    assert_eq!(v["lines"][0]["difference"], -2);
    assert_eq!(v["lines"][1]["changed_since_start"], false);
}

#[sqlx::test]
async fn post_twice_is_409(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    c.lines(&id, json!([{"product_id": p, "counted": 2}])).await;
    c.post(&id).await;
    let r = c.post(&id).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "ALREADY_POSTED");
    assert_eq!(c.moves(&p).await.len(), 1);
}

#[sqlx::test]
async fn posted_count_cannot_be_edited(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    c.lines(&id, json!([{"product_id": p, "counted": 2}])).await;
    c.post(&id).await;
    let path = format!("/api/v1/stock-counts/{id}");
    let r = c.lines(&id, json!([{"product_id": p, "counted": 9}])).await;
    assert_eq!(
        (r.status(), r.json()["error"]["code"].clone()),
        (409, json!("NOT_DRAFT"))
    );
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
    let r =
        c.a.delete_with(&path, Some(&c.s.cookie), Some(&c.s.csrf))
            .await;
    assert_eq!(
        (r.status(), r.json()["error"]["code"].clone()),
        (409, json!("NOT_DRAFT"))
    );
}

#[sqlx::test]
async fn bad_count_lines_are_field_errors(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    for (lines, field) in [
        (
            json!([{"product_id": p, "counted": -1}]),
            "lines[0].counted",
        ),
        (
            json!([{"product_id": p, "counted": 1, "unit_cost": "1.234"}]),
            "lines[0].unit_cost",
        ),
    ] {
        let r = c.lines(&id, lines).await;
        assert_eq!(r.status(), 422);
        assert_eq!(r.json()["error"]["details"]["field"], field);
    }
}

#[sqlx::test]
async fn archived_product_blocks_post(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    c.lines(&id, json!([{"product_id": p, "counted": 2}])).await;
    c.a.post_empty(&format!("/api/v1/products/{p}/archive"), &c.s)
        .await;
    let r = c.post(&id).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["details"]["field"], "lines[0].product_id");
    assert!(c.moves(&p).await.is_empty());
}

#[sqlx::test]
async fn cashier_cannot_post_count(pool: PgPool) {
    let c = ctx(pool.clone()).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    c.lines(&id, json!([{"product_id": p, "counted": 2}])).await;
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let s = c.a.login("cash", PW).await;
    assert_eq!(
        c.a.post_empty(&format!("/api/v1/stock-counts/{id}/post"), &s)
            .await
            .status(),
        403
    );
    assert_eq!(
        c.a.get_with(&format!("/api/v1/stock-counts/{id}"), Some(&s.cookie))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn post_is_audited_and_listed(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    c.posted(json!([{"product_id": p, "counted": 2}])).await;
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'stock_count.post'")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
    let list =
        c.a.get_with("/api/v1/stock-counts", Some(&c.s.cookie))
            .await
            .json();
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["line_count"], 1);
    assert_eq!(list["items"][0]["note"], "Shelf A");
}

#[sqlx::test]
async fn lines_say_whether_this_is_the_first_count(pool: PgPool) {
    let c = ctx(pool).await;
    let p = c.a.product(&c.s, "B 6143-8", "Bowl").await;
    let id = c.count().await;
    let v = c
        .lines(&id, json!([{"product_id": p, "counted": 12}]))
        .await
        .json();
    assert_eq!(v["lines"][0]["first_count"], true);
    c.post(&id).await;
    let id = c.count().await;
    let v = c
        .lines(&id, json!([{"product_id": p, "counted": 10}]))
        .await
        .json();
    assert_eq!(v["lines"][0]["first_count"], false);
}
