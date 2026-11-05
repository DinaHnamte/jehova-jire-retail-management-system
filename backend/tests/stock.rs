//! Stock list, product stock, movement history, adjustments (spec §2 rule 5; plan Task 7).
mod common;
use common::{Session, TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn boss(pool: &PgPool) -> (TestApp, Session) {
    seed_user(pool, "boss", PW, &["manager"]).await;
    let a = app(pool.clone());
    let s = a.login("boss", PW).await;
    (a, s)
}

/// Post an opening count for `lines` (product id, counted, cost).
async fn opening(a: &TestApp, s: &Session, lines: &[(&str, i64, Option<&str>)]) {
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
        .map(|(p, n, c)| json!({"product_id": p, "counted": n, "unit_cost": c}))
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

async fn adjust(a: &TestApp, s: &Session, p: &str, body: Value) -> common::TestResponse {
    a.post_json(
        &format!("/api/v1/products/{p}/adjustments"),
        body,
        Some(&s.cookie),
        Some(&s.csrf),
    )
    .await
}

fn names(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["product"]["name"].as_str().unwrap().to_string())
        .collect()
}

#[sqlx::test]
async fn stock_list_shows_on_hand_and_filters(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let pa = a.product(&s, "A 1", "Alpha").await;
    let _pb = a.product(&s, "B 2", "Beta").await;
    let pc = a.product(&s, "C 3", "Gamma").await;
    opening(&a, &s, &[(&pa, 5, Some("10")), (&pc, 4, None)]).await;
    let v = a.get_with("/api/v1/stock", Some(&s.cookie)).await.json();
    assert_eq!(names(&v), ["Alpha", "Beta", "Gamma"]);
    assert_eq!(v["items"][0]["on_hand"], 5);
    assert_eq!(v["items"][0]["avg_cost"], "10.0000");
    assert_eq!(v["items"][1]["on_hand"], 0);
    assert_eq!(v["items"][1]["avg_cost"], Value::Null);
    let v = a
        .get_with("/api/v1/stock?filter=out", Some(&s.cookie))
        .await
        .json();
    assert_eq!(names(&v), ["Beta"]);
    let v = a
        .get_with("/api/v1/stock?filter=unknown_cost", Some(&s.cookie))
        .await
        .json();
    assert_eq!(names(&v), ["Gamma"]);
}

#[sqlx::test]
async fn stock_list_search_by_partial_code(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    a.product(&s, "IP 1060-11", "Plate").await;
    a.product(&s, "B 6143-8", "Bowl").await;
    let v = a
        .get_with("/api/v1/stock?q=614", Some(&s.cookie))
        .await
        .json();
    assert_eq!(names(&v)[0], "Bowl");
    assert_eq!(v["total"], 1);
}

#[sqlx::test]
async fn stock_list_is_paged(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    for i in 0..30 {
        a.product(&s, &format!("Z {i}"), &format!("Item {i:02}"))
            .await;
    }
    let v = a
        .get_with("/api/v1/stock?page=2", Some(&s.cookie))
        .await
        .json();
    assert_eq!(
        (v["total"].clone(), v["page"].clone(), v["per_page"].clone()),
        (json!(30), json!(2), json!(25))
    );
    assert_eq!(v["items"].as_array().unwrap().len(), 5);
}

#[sqlx::test]
async fn damage_reduces_stock_and_is_audited(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    opening(&a, &s, &[(&p, 5, Some("10"))]).await;
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "damage", "quantity": 2, "reason": "Cracked in transit"}),
    )
    .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    assert_eq!(r.json()["on_hand"], 3);
    assert_eq!(r.json()["stock_value"], "30.00");
    let (src, ent): (uuid::Uuid, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT m.source_id, a.entity_id FROM stock_movements m
         LEFT JOIN audit_logs a ON a.action = 'stock.adjust' AND a.entity_id = m.source_id
         WHERE m.movement_type = 'damage'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        Some(src),
        ent,
        "the audit row is keyed by the movement's source id"
    );
}

#[sqlx::test]
async fn adjust_below_zero_is_422_insufficient_stock(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    opening(&a, &s, &[(&p, 3, None)]).await;
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "adjustment_out", "quantity": 4, "reason": "Recount"}),
    )
    .await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json()["error"]["code"], "INSUFFICIENT_STOCK");
    assert_eq!(
        r.json()["error"]["message"],
        "Only 3 of B 6143-8 in stock. Enter 3 or less."
    );
}

#[sqlx::test]
async fn adjust_needs_reason_and_valid_type(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "adjustment_in", "quantity": 1, "reason": " ab "}),
    )
    .await;
    assert_eq!(
        (r.status(), r.json()["error"]["details"]["field"].clone()),
        (422, json!("reason"))
    );
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "opening_balance", "quantity": 1, "reason": "abc"}),
    )
    .await;
    assert_eq!(
        (r.status(), r.json()["error"]["details"]["field"].clone()),
        (422, json!("type"))
    );
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "adjustment_in", "quantity": 0, "reason": "abc"}),
    )
    .await;
    assert_eq!(
        (r.status(), r.json()["error"]["details"]["field"].clone()),
        (422, json!("quantity"))
    );
    a.post_empty(&format!("/api/v1/products/{p}/archive"), &s)
        .await;
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "adjustment_in", "quantity": 1, "reason": "abc"}),
    )
    .await;
    assert_eq!(r.status(), 422);
}

#[sqlx::test]
async fn adjustment_in_keeps_unknown_cost(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    let r = adjust(
        &a,
        &s,
        &p,
        json!({"type": "adjustment_in", "quantity": 2, "reason": "Found in back"}),
    )
    .await;
    assert_eq!(r.status(), 201);
    assert_eq!(
        r.json(),
        json!({"on_hand": 2, "avg_cost": null, "stock_value": null})
    );
}

#[sqlx::test]
async fn stock_operator_cannot_adjust(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    seed_user(&pool, "op", PW, &["stock_operator"]).await;
    let so = a.login("op", PW).await;
    let r = adjust(
        &a,
        &so,
        &p,
        json!({"type": "adjustment_in", "quantity": 1, "reason": "abc"}),
    )
    .await;
    assert_eq!(r.status(), 403);
    assert_eq!(
        a.get_with(&format!("/api/v1/products/{p}/stock"), Some(&so.cookie))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn movements_newest_first_with_actor_name(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    opening(&a, &s, &[(&p, 5, Some("10"))]).await;
    adjust(
        &a,
        &s,
        &p,
        json!({"type": "damage", "quantity": 1, "reason": "Chipped"}),
    )
    .await;
    let v = a
        .get_with(&format!("/api/v1/products/{p}/movements"), Some(&s.cookie))
        .await
        .json();
    assert_eq!(v["total"], 2);
    assert_eq!(v["items"][0]["movement_type"], "damage");
    assert_eq!(v["items"][0]["quantity"], -1);
    assert_eq!(v["items"][0]["reason"], "Chipped");
    assert_eq!(v["items"][0]["created_by_name"], "boss");
    assert_eq!(v["items"][1]["movement_type"], "opening_balance");
    assert_eq!(v["items"][1]["unit_cost"], "10.00");
}

#[sqlx::test]
async fn product_stock_for_never_stocked_product_is_zero_unknown(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    let p = a.product(&s, "B 6143-8", "Bowl").await;
    let r = a
        .get_with(&format!("/api/v1/products/{p}/stock"), Some(&s.cookie))
        .await;
    assert_eq!(
        r.json(),
        json!({"on_hand": 0, "avg_cost": null, "stock_value": null})
    );
    let r = a
        .get_with(
            &format!("/api/v1/products/{}/stock", uuid::Uuid::now_v7()),
            Some(&s.cookie),
        )
        .await;
    assert_eq!(r.status(), 404);
}

#[sqlx::test]
async fn cashier_can_view_stock(pool: PgPool) {
    let (a, s) = boss(&pool).await;
    a.product(&s, "B 6143-8", "Bowl").await;
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let c = a.login("cash", PW).await;
    assert_eq!(
        a.get_with("/api/v1/stock", Some(&c.cookie)).await.status(),
        200
    );
}
