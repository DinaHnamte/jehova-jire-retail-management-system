//! The driver's own phone: /me/deliveries (Phase 6a Task 7).
mod common;
use common::shop::*;
use common::{Session, TestResponse};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const DPW: &str = "driver password 1";

async fn job(sh: &Shop, pool: &PgPool, fee: &str, paid: &str) -> String {
    let o = delivery_sale(sh, fee, paid).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM deliveries WHERE order_id = $1")
        .bind(uuid(&o["id"]))
        .fetch_one(pool)
        .await
        .unwrap();
    id.to_string()
}

async fn new_driver(sh: &Shop, username: &str, phone: &str) -> (String, Session) {
    let r =
        sh.a.post_json(
            "/api/v1/drivers",
            json!({"display_name": username, "username": username, "password": DPW,
                   "phone": phone, "default_pay": "40"}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    let id = r.json()["id"].as_str().unwrap().to_string();
    (id, sh.a.login(username, DPW).await)
}

async fn assign(sh: &Shop, did: &str, driver: &str) {
    let r =
        sh.a.post_json(
            &format!("/api/v1/deliveries/{did}/assign"),
            json!({"driver_id": driver}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
}

async fn mine(sh: &Shop, s: &Session) -> Value {
    let r =
        sh.a.get_with("/api/v1/me/deliveries", Some(&s.cookie))
            .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    r.json()
}

async fn one(sh: &Shop, s: &Session, did: &str) -> TestResponse {
    sh.a.get_with(&format!("/api/v1/me/deliveries/{did}"), Some(&s.cookie))
        .await
}

async fn step(sh: &Shop, s: &Session, did: &str, body: Value) -> TestResponse {
    sh.a.post_json(
        &format!("/api/v1/me/deliveries/{did}/status"),
        body,
        Some(&s.cookie),
        Some(&s.csrf),
    )
    .await
}

fn keys(v: &Value) -> Vec<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

fn no_price_or_cost(v: &Value) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                assert!(!k.contains("price") && !k.contains("cost"), "key {k}");
                no_price_or_cost(v);
            }
        }
        Value::Array(a) => a.iter().for_each(no_price_or_cost),
        _ => {}
    }
}

#[sqlx::test]
async fn driver_sees_only_own_active_jobs(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let (b, sb) = new_driver(&sh, "bina", "9862000002").await;
    let ja = job(&sh, &pool, "30", "0").await;
    let jb = job(&sh, &pool, "30", "0").await;
    let _unassigned = job(&sh, &pool, "30", "0").await;
    assign(&sh, &ja, &a).await;
    assign(&sh, &jb, &b).await;
    let v = mine(&sh, &sa).await;
    let active = v["active"].as_array().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["id"], ja);
    assert_eq!(active[0]["status"], "assigned");
    assert_eq!(active[0]["locality"], "Chanmari");
    assert_eq!(active[0]["recipient_name"], "Lalthangi");
    assert_eq!(active[0]["amount_to_collect"], "130.00");
    assert!(active[0]["order_number"].is_string());
    assert_eq!(v["history"].as_array().unwrap().len(), 0);
    let vb = mine(&sh, &sb).await;
    assert_eq!(vb["active"][0]["id"], jb);
}

#[sqlx::test]
async fn driver_cannot_see_other_drivers_job(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, _sa) = new_driver(&sh, "asha", "9862000001").await;
    let (_b, sb) = new_driver(&sh, "bina", "9862000002").await;
    let ja = job(&sh, &pool, "30", "0").await;
    assign(&sh, &ja, &a).await;
    let r = one(&sh, &sb, &ja).await;
    assert_eq!(r.status(), 404);
    let p = step(&sh, &sb, &ja, json!({"to": "picked_up"})).await;
    assert_eq!(p.status(), 404);
    assert_eq!(p.json()["error"]["message"], r.json()["error"]["message"]);
    // An id that does not exist looks the same.
    let none = step(
        &sh,
        &sb,
        &jj_retail_api::shared::ids::new_id().to_string(),
        json!({"to": "picked_up"}),
    )
    .await;
    assert_eq!(
        none.json()["error"]["message"],
        p.json()["error"]["message"]
    );
}

#[sqlx::test]
async fn reassigned_driver_gets_404(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let (b, _sb) = new_driver(&sh, "bina", "9862000002").await;
    let j = job(&sh, &pool, "30", "0").await;
    assign(&sh, &j, &a).await;
    assign(&sh, &j, &b).await;
    assert_eq!(
        step(&sh, &sa, &j, json!({"to": "picked_up"}))
            .await
            .status(),
        404
    );
    let r = one(&sh, &sa, &j).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert!(v.get("recipient_phone").is_none());
    assert!(v.get("address_line").is_none());
    assert_eq!(v["status"], "assigned");
    assert_eq!(mine(&sh, &sa).await["active"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn delivered_job_moves_to_history_without_address(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let j = job(&sh, &pool, "30", "50").await;
    assign(&sh, &j, &a).await;
    assert_eq!(
        step(&sh, &sa, &j, json!({"to": "picked_up"}))
            .await
            .status(),
        200
    );
    let r = step(
        &sh,
        &sa,
        &j,
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let v = r.json();
    assert!(v.get("address_line").is_none() && v.get("recipient_phone").is_none());
    assert_eq!(v["status"], "delivered");
    let l = mine(&sh, &sa).await;
    assert_eq!(l["active"].as_array().unwrap().len(), 0);
    let h = l["history"].as_array().unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0]["id"], j);
    assert_eq!(h[0]["status"], "delivered");
    assert_eq!(h[0]["locality"], "Chanmari");
    assert_eq!(h[0]["amount"], "130.00");
    assert!(h[0]["at"].is_string() && h[0]["order_number"].is_string());
    let mut k = keys(&h[0]);
    k.sort();
    assert_eq!(
        k,
        ["amount", "at", "id", "locality", "order_number", "status"]
    );
}

#[sqlx::test]
async fn driver_full_flow(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let j = job(&sh, &pool, "30", "0").await;
    assign(&sh, &j, &a).await;
    let d = one(&sh, &sa, &j).await.json();
    assert_eq!(d["status"], "assigned");
    let p = step(&sh, &sa, &j, json!({"to": "picked_up"})).await;
    assert_eq!(p.json()["status"], "picked_up");
    let r = step(
        &sh,
        &sa,
        &j,
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    let actors: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT to_status, actor_id FROM delivery_events
         WHERE delivery_id = $1::uuid AND kind = 'status' AND to_status IN ('picked_up', 'delivered')
         ORDER BY occurred_at, id",
    )
    .bind(&j)
    .fetch_all(&pool)
    .await
    .unwrap();
    let me: Uuid = a.parse().unwrap();
    assert_eq!(
        actors,
        [("picked_up".to_string(), me), ("delivered".to_string(), me)]
    );
    // Validation errors come through as for the owner.
    let j2 = job(&sh, &pool, "30", "0").await;
    assign(&sh, &j2, &a).await;
    let bad = step(&sh, &sa, &j2, json!({"to": "delivered"})).await;
    assert_eq!(bad.status(), 422);
}

#[sqlx::test]
async fn driver_double_tap_is_409_with_status(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let j = job(&sh, &pool, "30", "0").await;
    assign(&sh, &j, &a).await;
    assert_eq!(
        step(&sh, &sa, &j, json!({"to": "picked_up"}))
            .await
            .status(),
        200
    );
    let again = step(&sh, &sa, &j, json!({"to": "picked_up"})).await;
    assert_eq!(again.status(), 409, "{:?}", again.json());
    assert_eq!(again.json()["error"]["code"], "DELIVERY_STATUS");
    assert_eq!(again.json()["error"]["details"]["status"], "picked_up");
    let body = json!({"to": "delivered", "received_by": "Mary"});
    assert_eq!(step(&sh, &sa, &j, body.clone()).await.status(), 200);
    let again = step(&sh, &sa, &j, body).await;
    assert_eq!(again.status(), 409, "{:?}", again.json());
    assert_eq!(again.json()["error"]["code"], "DELIVERY_STATUS");
    assert_eq!(again.json()["error"]["details"]["status"], "delivered");
}

#[sqlx::test]
async fn driver_detail_has_no_prices(pool: PgPool) {
    let sh = shop(&pool).await;
    let (a, sa) = new_driver(&sh, "asha", "9862000001").await;
    let j = job(&sh, &pool, "30", "0").await;
    assign(&sh, &j, &a).await;
    let v = one(&sh, &sa, &j).await.json();
    no_price_or_cost(&v);
    no_price_or_cost(&mine(&sh, &sa).await);
    assert_eq!(v["recipient_phone"], "9862000000");
    assert_eq!(v["address_line"], "House 12, Chanmari West");
    assert_eq!(v["landmark"], "near the church");
    assert_eq!(v["instructions"], "call before coming");
    assert_eq!(v["collector"], "driver");
    assert_eq!(v["pay_method"], "retain_from_collection");
    assert_eq!(v["driver_pay"], "40.00");
    assert_eq!(v["amount_to_collect"], "130.00");
    assert!(v["maps_url"].as_str().unwrap().starts_with("https://"));
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let mut k = keys(&items[0]);
    k.sort();
    assert_eq!(k, ["code", "name", "quantity"]);
    assert_eq!(items[0]["name"], "Plate");
    assert_eq!(items[0]["quantity"], 1);
    for banned in ["fee", "order", "events", "driver", "customer"] {
        assert!(v.get(banned).is_none(), "{banned}");
    }
}

#[sqlx::test]
async fn staff_endpoints_forbidden_for_driver(pool: PgPool) {
    let sh = shop(&pool).await;
    let (_a, sa) = new_driver(&sh, "asha", "9862000001").await;
    for path in ["/api/v1/orders", "/api/v1/deliveries", "/api/v1/customers"] {
        let r = sh.a.get_with(path, Some(&sa.cookie)).await;
        assert_eq!(r.status(), 403, "{path}");
    }
    // And staff cannot use the driver's endpoints without the own-job permissions.
    let r =
        sh.a.get_with("/api/v1/me/deliveries", Some(&sh.c.cookie))
            .await;
    assert_eq!(r.status(), 403);
}
