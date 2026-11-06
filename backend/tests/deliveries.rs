//! The owner's dispatch API: list, detail, assign, edit, status, retry (Phase 6a Task 5).
mod common;
use axum::{body::Body, http::Request};
use common::shop::*;
use common::{Session, TestResponse};
use jj_retail_api::{
    deliveries::{actions, input::StatusChange},
    error::AppError,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A delivery sale of `qty` Plates for `fee`, `paid` in cash up front, with `driver_pay` typed
/// at checkout or not. A new customer Mary unless `customer` names one.
async fn sale(
    sh: &Shop,
    qty: i64,
    fee: &str,
    paid: &str,
    driver_pay: Option<&str>,
    customer: Option<&str>,
) -> Value {
    let mut dl = json!({
        "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
        "address_line": "House 12, Chanmari West", "locality": "Chanmari",
        "landmark": "near the church", "instructions": "call before coming",
        "fee": fee
    });
    if let Some(p) = driver_pay {
        dl["driver_pay"] = json!(p);
    }
    let mut body = json!({"lines": [{"product_id": sh.p, "quantity": qty}], "delivery": dl});
    match customer {
        Some(c) => body["customer_id"] = json!(c),
        None => body["new_customer"] = json!({"phone": "9862012345", "name": "Mary"}),
    }
    if paid != "0" {
        body["payments"] = json!([{"method": "cash", "amount": paid}]);
    }
    sell(sh, body).await
}

async fn delivery_of(pool: &PgPool, order: &Value) -> String {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM deliveries WHERE order_id = $1")
        .bind(uuid(&order["id"]))
        .fetch_one(pool)
        .await
        .unwrap();
    id.to_string()
}

/// A driver added through the API; returns the driver's user id.
async fn driver(sh: &Shop, username: &str, name: &str, phone: &str, pay: &str) -> String {
    let r =
        sh.a.post_json(
            "/api/v1/drivers",
            json!({"display_name": name, "username": username, "password": "driver password 1",
                   "phone": phone, "default_pay": pay}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 201, "{:?}", r.json());
    r.json()["id"].as_str().unwrap().to_string()
}

async fn deactivate(sh: &Shop, driver: &str) {
    let r =
        sh.a.patch_json(
            &format!("/api/v1/drivers/{driver}"),
            json!({"is_active": false}),
            Some(&sh.m.cookie),
            Some(&sh.m.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
}

async fn act(sh: &Shop, s: &Session, did: &str, what: &str, body: Value) -> TestResponse {
    sh.a.post_json(
        &format!("/api/v1/deliveries/{did}/{what}"),
        body,
        Some(&s.cookie),
        Some(&s.csrf),
    )
    .await
}

async fn assign(sh: &Shop, did: &str, body: Value) -> TestResponse {
    act(sh, &sh.m, did, "assign", body).await
}

async fn status(sh: &Shop, did: &str, body: Value) -> TestResponse {
    act(sh, &sh.m, did, "status", body).await
}

async fn ok(r: TestResponse) -> Value {
    assert_eq!(r.status(), 200, "{:?}", r.json());
    r.json()
}

async fn edit(sh: &Shop, did: &str, key: Option<&str>, body: Value) -> TestResponse {
    let mut b = Request::patch(format!("/api/v1/deliveries/{did}"))
        .header("content-type", "application/json")
        .header("cookie", &sh.m.cookie)
        .header("x-csrf-token", &sh.m.csrf);
    if let Some(k) = key {
        b = b.header("idempotency-key", k);
    }
    sh.a.send(b.body(Body::from(body.to_string())).unwrap())
        .await
}

async fn get(sh: &Shop, path: &str) -> Value {
    let r = sh.a.get_with(path, Some(&sh.m.cookie)).await;
    assert_eq!(r.status(), 200, "{:?}", r.json());
    r.json()
}

async fn detail(sh: &Shop, did: &str) -> Value {
    get(sh, &format!("/api/v1/deliveries/{did}")).await
}

async fn tab(sh: &Shop, t: &str) -> Value {
    get(sh, &format!("/api/v1/deliveries?status={t}")).await
}

async fn scalar_i64(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

fn field(r: &TestResponse) -> String {
    r.json()["error"]["details"]["field"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

#[sqlx::test]
async fn assign_shows_job_and_prefills_pay_from_driver_default(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let lal = driver(&sh, "lal", "Lalbiaka", "9862000001", "40").await;

    // A cashier can read deliveries but not assign them.
    let r = act(&sh, &sh.c, &did, "assign", json!({"driver_id": lal})).await;
    assert_eq!(r.status(), 403);

    let v = ok(assign(&sh, &did, json!({"driver_id": lal})).await).await;
    assert_eq!(v["status"], "assigned");
    assert_eq!(v["driver_pay"], "40.00");
    assert_eq!(v["driver"]["id"], lal);
    assert_eq!(v["driver"]["name"], "Lalbiaka");
    assert!(v["assigned_at"].is_string());
    let out = tab(&sh, "out").await;
    assert_eq!(out["items"].as_array().unwrap().len(), 1);
    assert_eq!(out["items"][0]["id"], did);
    assert_eq!(out["items"][0]["driver"]["name"], "Lalbiaka");
    assert_eq!(out["items"][0]["driver"]["active"], true);
    let events = v["events"].as_array().unwrap();
    assert_eq!(events.last().unwrap()["from"], "to_send");
    assert_eq!(events.last().unwrap()["to"], "assigned");
    let audits = scalar_i64(
        &pool,
        "SELECT count(*) FROM audit_logs WHERE action = 'delivery.assign'",
    )
    .await;
    assert_eq!(audits, 1);
}

#[sqlx::test]
async fn assign_keeps_pay_typed_at_checkout(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", Some("30"), None).await;
    let did = delivery_of(&pool, &o).await;
    let lal = driver(&sh, "lal", "Lalbiaka", "9862000001", "55").await;
    let v = ok(assign(&sh, &did, json!({"driver_id": lal})).await).await;
    assert_eq!(v["driver_pay"], "30.00");

    // Pay typed on the assignment wins over both.
    let cid = o["customer"]["id"].as_str().unwrap().to_string();
    let o2 = sale(&sh, 1, "50", "0", Some("30"), Some(&cid)).await;
    let did2 = delivery_of(&pool, &o2).await;
    let v = ok(assign(&sh, &did2, json!({"driver_id": lal, "driver_pay": "70"})).await).await;
    assert_eq!(v["driver_pay"], "70.00");
    let r = assign(&sh, &did2, json!({"driver_id": lal, "driver_pay": "-5"})).await;
    assert_eq!(r.status(), 422);
}

#[sqlx::test]
async fn reassign_ends_previous_assignment(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let b = driver(&sh, "b", "Mama", "9862000002", "45").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;

    let r = assign(&sh, &did, json!({"driver_id": a})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(field(&r), "driver_id");
    assert_eq!(r.json()["error"]["message"], "This driver already has it.");

    let v = ok(assign(&sh, &did, json!({"driver_id": b})).await).await;
    assert_eq!(v["status"], "assigned");
    assert_eq!(v["driver"]["id"], b);
    // The pay set at the first assignment is kept.
    assert_eq!(v["driver_pay"], "40.00");
    let rows: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT driver_id, end_reason FROM delivery_assignments ORDER BY assigned_at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0.to_string(), a);
    assert_eq!(rows[0].1.as_deref(), Some("reassigned"));
    assert_eq!(rows[1].0.to_string(), b);
    assert_eq!(rows[1].1, None);
}

#[sqlx::test]
async fn assign_inactive_driver_is_rejected(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    deactivate(&sh, &a).await;
    let r = assign(&sh, &did, json!({"driver_id": a})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(field(&r), "driver_id");
    assert_eq!(r.json()["error"]["message"], "Pick an active driver.");
    // A user who isn't a driver is no better.
    let r = assign(&sh, &did, json!({"driver_id": sh.m_id})).await;
    assert_eq!(field(&r), "driver_id");
    assert_eq!(
        scalar_i64(&pool, "SELECT count(*) FROM delivery_assignments").await,
        0
    );
    assert_eq!(detail(&sh, &did).await["status"], "to_send");
}

#[sqlx::test]
async fn list_flags_inactive_driver(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    deactivate(&sh, &a).await;
    let out = tab(&sh, "out").await;
    assert_eq!(out["items"][0]["driver"]["active"], false);
    assert_eq!(detail(&sh, &did).await["driver"]["active"], false);
}

#[sqlx::test]
async fn list_tabs_and_counts(pool: PgPool) {
    let sh = shop(&pool).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let first = sale(&sh, 1, "50", "0", None, None).await;
    let cid = first["customer"]["id"].as_str().unwrap().to_string();
    let mut ids = vec![delivery_of(&pool, &first).await];
    for _ in 0..4 {
        let o = sale(&sh, 1, "50", "0", None, Some(&cid)).await;
        ids.push(delivery_of(&pool, &o).await);
    }
    // ids[0] stays to send; [1] assigned; [2] picked up; [3] failed; [4] delivered.
    for did in &ids[1..] {
        ok(assign(&sh, did, json!({"driver_id": a})).await).await;
    }
    for did in &ids[2..] {
        ok(status(&sh, did, json!({"to": "picked_up"})).await).await;
    }
    ok(status(
        &sh,
        &ids[3],
        json!({"to": "failed", "reason": "address_not_found"}),
    )
    .await)
    .await;
    ok(status(
        &sh,
        &ids[4],
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await)
    .await;

    let want_counts = json!({"to_send": 1, "out": 2, "failed": 1});
    let ts = tab(&sh, "to_send").await;
    assert_eq!(ts["counts"], want_counts);
    let items = ts["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let it = &items[0];
    assert_eq!(it["id"], ids[0]);
    assert_eq!(it["order_id"], first["id"]);
    assert_eq!(it["order_number"], first["number"]);
    assert_eq!(it["status"], "to_send");
    assert_eq!(it["recipient_name"], "Lalthangi");
    assert_eq!(it["locality"], "Chanmari");
    assert_eq!(it["driver"], Value::Null);
    assert_eq!(it["amount_to_collect"], "150.00");
    assert_eq!(it["fee"], "50.00");
    assert!(it["since"].is_string());
    assert_eq!(it["fail_reason"], Value::Null);

    let out = tab(&sh, "out").await;
    assert_eq!(out["counts"], want_counts);
    let out_ids: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    // Oldest status change first: [1] was assigned before [2] was picked up.
    assert_eq!(out_ids, [ids[1].as_str(), ids[2].as_str()]);

    let failed = tab(&sh, "failed").await;
    assert_eq!(failed["items"].as_array().unwrap().len(), 1);
    assert_eq!(failed["items"][0]["fail_reason"], "address_not_found");
    assert_eq!(failed["counts"], want_counts);

    // Done is newest first: cancel nothing here, so deliver [1] too and it comes first.
    ok(status(&sh, &ids[1], json!({"to": "picked_up"})).await).await;
    ok(status(
        &sh,
        &ids[1],
        json!({"to": "delivered", "received_by": "Mary"}),
    )
    .await)
    .await;
    let done = tab(&sh, "done").await;
    let done_ids: Vec<&str> = done["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(done_ids, [ids[1].as_str(), ids[4].as_str()]);
    assert_eq!(done["counts"], json!({"to_send": 1, "out": 1, "failed": 1}));

    // Older than 7 days drops off the Done tab.
    sqlx::query("UPDATE deliveries SET delivered_at = now() - interval '8 days' WHERE id = $1")
        .bind(Uuid::parse_str(&ids[4]).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(tab(&sh, "done").await["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn status_out_of_order_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    let r = status(&sh, &did, json!({"to": "delivered", "received_by": "Mary"})).await;
    assert_eq!(r.status(), 409);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "DELIVERY_STATUS");
    assert_eq!(e["details"]["status"], "assigned");
    assert_eq!(
        e["message"],
        "This delivery is assigned to a driver. That step isn't possible now."
    );
    // A double tap of a step already done is 409 naming the status it is now.
    ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;
    let r = status(&sh, &did, json!({"to": "picked_up"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["details"]["status"], "picked_up");
    let r = status(&sh, &did, json!({"to": "returned"})).await;
    assert_eq!(r.status(), 422);
    assert_eq!(field(&r), "to");
}

#[sqlx::test]
async fn owner_marks_delivered_for_driver(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    let v = ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;
    assert_eq!(v["status"], "picked_up");
    assert!(v["picked_up_at"].is_string());
    let v = ok(status(
        &sh,
        &did,
        json!({"to": "delivered", "received_by": "  Mary  "}),
    )
    .await)
    .await;
    assert_eq!(v["status"], "delivered");
    assert_eq!(v["received_by"], "Mary");
    assert!(v["delivered_at"].is_string());
    assert_eq!(v["driver"], Value::Null);
    let last = v["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["from"], "picked_up");
    assert_eq!(last["to"], "delivered");
    assert_eq!(last["kind"], "status");
    assert_eq!(last["actor_name"], "boss");
    assert_eq!(last["note"], "Marked by boss for Lalbiaka");
    let actor: Uuid =
        sqlx::query_scalar("SELECT actor_id FROM delivery_events WHERE to_status = 'delivered'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(actor, sh.m_id);
    let end: String = sqlx::query_scalar("SELECT end_reason FROM delivery_assignments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(end, "completed");
}

#[sqlx::test]
async fn failed_needs_reason_and_other_needs_note(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;

    let r = status(&sh, &did, json!({"to": "failed"})).await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "reason"));
    let r = status(
        &sh,
        &did,
        json!({"to": "failed", "reason": "other", "note": " "}),
    )
    .await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "note"));
    let r = status(&sh, &did, json!({"to": "delivered"})).await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "received_by"));
    assert_eq!(r.json()["error"]["message"], "Write who received it.");
    let long = "x".repeat(81);
    let r = status(&sh, &did, json!({"to": "delivered", "received_by": long})).await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "received_by"));
    assert_eq!(detail(&sh, &did).await["status"], "picked_up");

    let v = ok(status(
        &sh,
        &did,
        json!({"to": "failed", "reason": "other", "note": "Gate locked"}),
    )
    .await)
    .await;
    assert_eq!(v["status"], "failed");
    assert_eq!(v["driver"], Value::Null);
    let at = &v["attempts"][0];
    assert_eq!(at["driver_name"], "Lalbiaka");
    assert_eq!(at["reason"], "other");
    assert_eq!(at["note"], "Gate locked");
    assert_eq!(at["trip_pay"], Value::Null);
    let end: String = sqlx::query_scalar("SELECT end_reason FROM delivery_assignments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(end, "failed");
}

#[sqlx::test]
async fn retry_sets_trip_pay_and_reassigns(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let b = driver(&sh, "b", "Mama", "9862000002", "45").await;

    // Retry is only for a failed delivery.
    let r = act(&sh, &sh.m, &did, "retry", json!({"driver_id": b})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["details"]["status"], "to_send");

    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;
    ok(status(
        &sh,
        &did,
        json!({"to": "failed", "reason": "customer_unavailable"}),
    )
    .await)
    .await;
    let v = ok(act(
        &sh,
        &sh.m,
        &did,
        "retry",
        json!({"driver_id": b, "trip_pay": "20"}),
    )
    .await)
    .await;
    assert_eq!(v["status"], "assigned");
    assert_eq!(v["driver"]["id"], b);
    assert_eq!(v["attempts"][0]["trip_pay"], "20.00");
    assert_eq!(v["attempts"][0]["driver_name"], "Lalbiaka");
    let open: Vec<Uuid> =
        sqlx::query_scalar("SELECT driver_id FROM delivery_assignments WHERE ended_at IS NULL")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].to_string(), b);

    // A second failure and a retry without trip pay records ₹0 for that trip.
    ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;
    ok(status(
        &sh,
        &did,
        json!({"to": "failed", "reason": "customer_refused"}),
    )
    .await)
    .await;
    let v = ok(act(&sh, &sh.m, &did, "retry", json!({"driver_id": b})).await).await;
    let pays: Vec<Option<Decimal>> =
        sqlx::query_scalar("SELECT trip_pay FROM delivery_attempts ORDER BY occurred_at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(pays, [Some(d("20")), Some(d("0"))]);
    assert_eq!(v["attempts"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn fee_increase_after_full_payment_leaves_difference_owed(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 2, "50", "250", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let sale_entries = |pool: PgPool| async move {
        let v: Vec<(String, Decimal)> = sqlx::query_as(
            "SELECT e.account, e.amount FROM ledger_entries e
             JOIN ledger_transactions t ON t.id = e.transaction_id
             WHERE t.kind = 'sale' ORDER BY e.account",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        v
    };
    let before = sale_entries(pool.clone()).await;

    // The key is required.
    let r = edit(&sh, &did, None, json!({"fee": "80"})).await;
    assert_eq!(r.status(), 400);

    let k = key();
    let r = edit(&sh, &did, Some(&k), json!({"fee": "80"})).await;
    let v = ok(r).await;
    assert_eq!(v["fee"], "80.00");
    assert_eq!(v["order"]["total"], "280.00");
    assert_eq!(v["order"]["paid"], "250.00");
    assert_eq!(v["order"]["owed"], "30.00");
    assert_eq!(v["amount_to_collect"], "30.00");
    assert_eq!(order_owed(&pool, &o).await, d("30"));
    // A retry with the same key changes nothing more.
    ok(edit(&sh, &did, Some(&k), json!({"fee": "80"})).await).await;
    let r = edit(&sh, &did, Some(&k), json!({"fee": "90"})).await;
    assert_eq!(r.status(), 409);

    let adj: Vec<(String, Decimal)> = sqlx::query_as(
        "SELECT e.account, e.amount FROM ledger_entries e
         JOIN ledger_transactions t ON t.id = e.transaction_id
         WHERE t.kind = 'fee_adjustment' ORDER BY e.account",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        adj,
        [
            ("delivery_fee_income".to_string(), d("-30")),
            ("receivable".to_string(), d("30"))
        ]
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'fee_adjustment'"
        )
        .await,
        1
    );
    assert_eq!(sale_entries(pool.clone()).await, before);
    assert_eq!(account_total(&pool, "delivery_fee_income").await, d("-80"));
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn fee_cut_below_paid_is_rejected(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 2, "50", "250", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let r = edit(
        &sh,
        &did,
        Some(&key()),
        json!({"fee": "20", "locality": "Zarkawt"}),
    )
    .await;
    assert_eq!(r.status(), 422);
    assert_eq!(field(&r), "fee");
    assert_eq!(
        r.json()["error"]["message"],
        "The customer has already paid more than that. Cancel the delivery or return items to give money back."
    );
    let v = detail(&sh, &did).await;
    assert_eq!(
        (v["fee"].as_str(), v["locality"].as_str()),
        (Some("50.00"), Some("Chanmari"))
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'fee_adjustment'"
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT count(*) FROM delivery_events WHERE kind = 'edit'"
        )
        .await,
        0
    );

    // On an unpaid order the fee can come down; the difference is reversed.
    let cid = o["customer"]["id"].as_str().unwrap().to_string();
    let o2 = sale(&sh, 2, "50", "0", None, Some(&cid)).await;
    let did2 = delivery_of(&pool, &o2).await;
    let v = ok(edit(&sh, &did2, Some(&key()), json!({"fee": "20"})).await).await;
    assert_eq!(v["order"]["owed"], "220.00");
    assert_eq!(v["order"]["total"], "220.00");
    assert_eq!(account_total(&pool, "delivery_fee_income").await, d("-70"));
    ledger_is_sound(&pool).await;
}

#[sqlx::test]
async fn edit_after_pickup_is_409(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    // Still editable while assigned.
    ok(edit(
        &sh,
        &did,
        Some(&key()),
        json!({"instructions": "ring twice"}),
    )
    .await)
    .await;
    ok(status(&sh, &did, json!({"to": "picked_up"})).await).await;
    let r = edit(&sh, &did, Some(&key()), json!({"fee": "80"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["code"], "DELIVERY_STATUS");
    assert_eq!(r.json()["error"]["details"]["status"], "picked_up");
    assert_eq!(detail(&sh, &did).await["fee"], "50.00");
}

#[sqlx::test]
async fn edit_records_only_changed_fields(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;

    let r = edit(&sh, &did, Some(&key()), json!({"recipient_phone": "12345"})).await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "recipient_phone"));
    let r = edit(&sh, &did, Some(&key()), json!({"locality": "  "})).await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "locality"));
    let r = edit(
        &sh,
        &did,
        Some(&key()),
        json!({"collector": "shop", "pay_method": "retain_from_collection"}),
    )
    .await;
    assert_eq!((r.status(), field(&r).as_str()), (422, "pay_method"));

    let v = ok(edit(
        &sh,
        &did,
        Some(&key()),
        json!({"recipient_name": "Lalthangi", "locality": " Zarkawt ", "landmark": "",
               "recipient_phone": "+91 98620 00000", "fee": "50"}),
    )
    .await)
    .await;
    assert_eq!(v["locality"], "Zarkawt");
    assert_eq!(v["landmark"], Value::Null);
    let last = v["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["kind"], "edit");
    assert_eq!(last["from"], "to_send");
    assert_eq!(last["to"], "to_send");
    assert_eq!(
        last["changes"],
        json!({"locality": ["Chanmari", "Zarkawt"], "landmark": ["near the church", null]})
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT count(*) FROM ledger_transactions WHERE kind = 'fee_adjustment'"
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT count(*) FROM audit_logs WHERE action = 'delivery.edit'"
        )
        .await,
        1
    );

    // Letting the shop collect switches the driver to being paid by the shop.
    let v = ok(edit(&sh, &did, Some(&key()), json!({"collector": "shop"})).await).await;
    assert_eq!(v["collector"], "shop");
    assert_eq!(v["pay_method"], "paid_by_shop");
    assert_eq!(v["amount_to_collect"], "0.00");
    let last = v["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        last["changes"],
        json!({"collector": ["driver", "shop"], "pay_method": ["retain_from_collection", "paid_by_shop"]})
    );

    // Nothing changed: no new event.
    let n = scalar_i64(&pool, "SELECT count(*) FROM delivery_events").await;
    ok(edit(&sh, &did, Some(&key()), json!({"locality": "Zarkawt"})).await).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT count(*) FROM delivery_events").await,
        n
    );
}

#[sqlx::test]
async fn detail_whatsapp_and_maps_links(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 2, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let number = o["number"].as_str().unwrap().to_string();

    let v = detail(&sh, &did).await;
    assert_eq!(v["driver_whatsapp_url"], Value::Null);
    assert_eq!(v["customer_whatsapp_url"], Value::Null);
    assert_eq!(
        v["maps_url"],
        "https://www.google.com/maps/search/?api=1&query=House%2012%2C%20Chanmari%20West%2C%20Chanmari"
    );
    assert_eq!(
        v["items"],
        json!([{"name": "Plate", "code": null, "quantity": 2}])
    );
    assert_eq!(v["order"]["id"], o["id"]);
    assert_eq!(v["order"]["number"], number);
    assert_eq!(v["order"]["total"], "250.00");
    assert_eq!(v["order"]["paid"], "0.00");
    assert_eq!(v["order"]["owed"], "250.00");
    assert_eq!(v["amount_to_collect"], "250.00");
    assert_eq!(v["recipient_phone"], "9862000000");
    assert_eq!(v["collector"], "driver");
    assert_eq!(v["pay_method"], "retain_from_collection");

    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let v = ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    let du = v["driver_whatsapp_url"].as_str().unwrap();
    let prefix = "https://wa.me/919862000001?text=";
    assert!(du.starts_with(prefix), "{du}");
    let text = pct_decode(&du[prefix.len()..]);
    assert!(text.contains(&format!("*Delivery {number}*")), "{text}");
    assert!(text.contains("Lalthangi · 9862000000"), "{text}");
    assert!(text.contains("House 12, Chanmari West, Chanmari"), "{text}");
    assert!(text.contains("Landmark: near the church"), "{text}");
    assert!(text.contains("Note: call before coming"), "{text}");
    assert!(
        text.contains("Map: https://www.google.com/maps/search/"),
        "{text}"
    );
    assert!(text.contains("Collect: ₹250"), "{text}");
    assert!(
        text.contains(&format!("https://shop.test/driver/deliveries/{did}")),
        "{text}"
    );
    let cu = v["customer_whatsapp_url"].as_str().unwrap();
    let prefix = "https://wa.me/919862000000?text=";
    assert!(cu.starts_with(prefix), "{cu}");
    let text = pct_decode(&cu[prefix.len()..]);
    assert!(
        text.contains(&format!("Your order {number} from")),
        "{text}"
    );
    assert!(
        text.contains("on its way with Lalbiaka (9862000001)"),
        "{text}"
    );
    assert!(text.contains("Amount to pay: ₹250."), "{text}");
}

#[sqlx::test]
async fn concurrent_assign_leaves_one_open_assignment(pool: PgPool) {
    let sh = shop(&pool).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let b = driver(&sh, "b", "Mama", "9862000002", "45").await;
    let first = sale(&sh, 1, "50", "0", None, None).await;
    let cid = first["customer"]["id"].as_str().unwrap().to_string();
    // `join!` polls in argument order, so each round gives the other request a head start.
    for a_first in [true, false] {
        let o = sale(&sh, 1, "50", "0", None, Some(&cid)).await;
        let did = delivery_of(&pool, &o).await;
        let id = Uuid::parse_str(&did).unwrap();
        // Hold the row so both requests are in flight before either can take it.
        let mut gate = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM deliveries WHERE id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *gate)
            .await
            .unwrap();
        let release = async {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            gate.commit().await.unwrap();
        };
        let ra = assign(&sh, &did, json!({"driver_id": a}));
        let rb = assign(&sh, &did, json!({"driver_id": b}));
        let (ra, rb) = if a_first {
            let (ra, rb, ()) = tokio::join!(ra, rb, release);
            (ra, rb)
        } else {
            let (rb, ra, ()) = tokio::join!(rb, ra, release);
            (ra, rb)
        };
        assert_eq!(ra.status(), 200, "{:?}", ra.json());
        assert_eq!(rb.status(), 200, "{:?}", rb.json());
        let open: Vec<Uuid> = sqlx::query_scalar(
            "SELECT driver_id FROM delivery_assignments WHERE delivery_id = $1 AND ended_at IS NULL",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();
        let current: Option<Uuid> =
            sqlx::query_scalar("SELECT current_driver_id FROM deliveries WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(Some(open[0]), current);
        let total: i64 =
            sqlx::query_scalar("SELECT count(*) FROM delivery_assignments WHERE delivery_id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total, 2);
        // The second assign waited for the first and saw it: one assign, then one reassign.
        let froms: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT from_status FROM delivery_events
             WHERE delivery_id = $1 AND to_status = 'assigned' ORDER BY from_status",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            froms,
            [Some("assigned".to_string()), Some("to_send".to_string())]
        );
    }
}

#[sqlx::test]
async fn change_status_by_another_driver_is_404(pool: PgPool) {
    let sh = shop(&pool).await;
    let o = sale(&sh, 1, "50", "0", None, None).await;
    let did = delivery_of(&pool, &o).await;
    let a = driver(&sh, "a", "Lalbiaka", "9862000001", "40").await;
    let b = driver(&sh, "b", "Mama", "9862000002", "45").await;
    ok(assign(&sh, &did, json!({"driver_id": a})).await).await;
    let (did, a, b) = (
        Uuid::parse_str(&did).unwrap(),
        Uuid::parse_str(&a).unwrap(),
        Uuid::parse_str(&b).unwrap(),
    );

    let mut tx = pool.begin().await.unwrap();
    let r = actions::change_status(&mut tx, b, did, StatusChange::PickedUp, Some(b)).await;
    assert!(matches!(r, Err(AppError::NotFound)), "{r:?}");
    drop(tx);
    let mut tx = pool.begin().await.unwrap();
    actions::change_status(&mut tx, a, did, StatusChange::PickedUp, Some(a))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // The driver acting for themselves gets no "marked by" note.
    let note: Option<String> =
        sqlx::query_scalar("SELECT note FROM delivery_events WHERE to_status = 'picked_up'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(note, None);
}
