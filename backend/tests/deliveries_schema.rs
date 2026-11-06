//! Delivery schema invariants (Phase 6a): fee in total, status/driver/pay consistency,
//! one open assignment, immutable events, new ledger and stock values.
mod common;
use common::seed_user;
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_order(
    pool: &PgPool,
    u: Uuid,
    fulfilment: &str,
    total: &str,
    fee: &str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO orders (id, number, status, fulfilment, subtotal, delivery_fee, total, receipt_token, created_by)
         VALUES ($1, $1::text, 'confirmed', $2, 100, $4::numeric, $3::numeric, $1::text, $5)",
    )
    .bind(id)
    .bind(fulfilment)
    .bind(total)
    .bind(fee)
    .bind(u)
    .execute(pool)
    .await?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
async fn insert_delivery(
    pool: &PgPool,
    order: Uuid,
    status: &str,
    driver: Option<Uuid>,
    driver_pay: Option<&str>,
    collector: &str,
    pay_method: &str,
    received_by: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO deliveries (id, order_id, status, recipient_name, recipient_phone, address_line,
            locality, fee, driver_pay, collector, pay_method, current_driver_id, received_by)
         VALUES ($1, $2, $3, 'Lal', '9862000000', 'Main road', 'Chanmari', 50, $4::numeric, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(order)
    .bind(status)
    .bind(driver_pay)
    .bind(collector)
    .bind(pay_method)
    .bind(driver)
    .bind(received_by)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn seed_delivery_order(pool: &PgPool, u: Uuid) -> Uuid {
    insert_order(pool, u, "delivery", "150", "50")
        .await
        .unwrap()
}

#[sqlx::test]
async fn delivery_order_total_includes_fee(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    assert!(
        insert_order(&pool, u, "delivery", "100", "50")
            .await
            .is_err()
    );
    assert!(
        insert_order(&pool, u, "delivery", "150", "50")
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn counter_order_cannot_have_fee(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    assert!(
        insert_order(&pool, u, "counter", "110", "10")
            .await
            .is_err()
    );
    assert!(insert_order(&pool, u, "counter", "100", "0").await.is_ok());
}

#[sqlx::test]
async fn retain_requires_driver_collector(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_delivery_order(&pool, u).await;
    assert!(
        insert_delivery(
            &pool,
            o,
            "to_send",
            None,
            None,
            "shop",
            "retain_from_collection",
            None
        )
        .await
        .is_err()
    );
    assert!(
        insert_delivery(
            &pool,
            o,
            "to_send",
            None,
            None,
            "driver",
            "retain_from_collection",
            None
        )
        .await
        .is_ok()
    );
}

#[sqlx::test]
async fn assigned_needs_driver_and_pay(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let d = seed_user(&pool, "driver1", "correct horse battery", &[]).await;
    let o = seed_delivery_order(&pool, u).await;
    assert!(
        insert_delivery(
            &pool,
            o,
            "assigned",
            None,
            Some("30"),
            "shop",
            "paid_by_shop",
            None
        )
        .await
        .is_err()
    );
    assert!(
        insert_delivery(
            &pool,
            o,
            "assigned",
            Some(d),
            None,
            "shop",
            "paid_by_shop",
            None
        )
        .await
        .is_err()
    );
    assert!(
        insert_delivery(
            &pool,
            o,
            "assigned",
            Some(d),
            Some("30"),
            "shop",
            "paid_by_shop",
            None
        )
        .await
        .is_ok()
    );
}

#[sqlx::test]
async fn delivered_needs_received_by(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_delivery_order(&pool, u).await;
    // delivered holds no driver (current_driver_id only for assigned/picked_up).
    assert!(
        insert_delivery(
            &pool,
            o,
            "delivered",
            None,
            Some("30"),
            "shop",
            "paid_by_shop",
            None
        )
        .await
        .is_err()
    );
    assert!(
        insert_delivery(
            &pool,
            o,
            "to_send",
            None,
            None,
            "shop",
            "paid_by_shop",
            Some("Mum")
        )
        .await
        .is_err()
    );
    assert!(
        insert_delivery(
            &pool,
            o,
            "delivered",
            None,
            Some("30"),
            "shop",
            "paid_by_shop",
            Some("Mum")
        )
        .await
        .is_ok()
    );
}

#[sqlx::test]
async fn one_open_assignment_per_delivery(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let d = seed_user(&pool, "driver1", "correct horse battery", &[]).await;
    let o = seed_delivery_order(&pool, u).await;
    let del = insert_delivery(
        &pool,
        o,
        "to_send",
        None,
        None,
        "shop",
        "paid_by_shop",
        None,
    )
    .await
    .unwrap();
    let assign = |ended: bool| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO delivery_assignments (id, delivery_id, driver_id, assigned_by, ended_at, end_reason)
                 VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() END, CASE WHEN $5 THEN 'reassigned' END)",
            )
            .bind(Uuid::now_v7())
            .bind(del)
            .bind(d)
            .bind(u)
            .bind(ended)
            .execute(&pool)
            .await
        }
    };
    assert!(assign(false).await.is_ok());
    assert!(assign(false).await.is_err());
    assert!(assign(true).await.is_ok());
}

#[sqlx::test]
async fn failed_other_needs_note(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let d = seed_user(&pool, "driver1", "correct horse battery", &[]).await;
    let o = seed_delivery_order(&pool, u).await;
    let del = insert_delivery(
        &pool,
        o,
        "to_send",
        None,
        None,
        "shop",
        "paid_by_shop",
        None,
    )
    .await
    .unwrap();
    let attempt = |reason: &'static str, note: Option<&'static str>| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO delivery_attempts (id, delivery_id, driver_id, reason, note, recorded_by)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::now_v7())
            .bind(del)
            .bind(d)
            .bind(reason)
            .bind(note)
            .bind(u)
            .execute(&pool)
            .await
        }
    };
    assert!(attempt("other", None).await.is_err());
    assert!(attempt("other", Some("Gate locked")).await.is_ok());
    assert!(attempt("customer_unavailable", None).await.is_ok());
}

#[sqlx::test]
async fn delivery_events_cannot_be_changed(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_delivery_order(&pool, u).await;
    let del = insert_delivery(
        &pool,
        o,
        "to_send",
        None,
        None,
        "shop",
        "paid_by_shop",
        None,
    )
    .await
    .unwrap();
    let ev = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO delivery_events (id, delivery_id, from_status, to_status, kind, actor_id)
         VALUES ($1, $2, NULL, 'to_send', 'status', $3)",
    )
    .bind(ev)
    .bind(del)
    .bind(u)
    .execute(&pool)
    .await
    .unwrap();
    for sql in [
        "UPDATE delivery_events SET note = 'x'",
        "DELETE FROM delivery_events",
    ] {
        let e = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(e.to_string().contains("cannot be changed"), "{sql}: {e}");
    }
}

#[sqlx::test]
async fn fee_income_account_and_fee_adjustment_kind_allowed(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_delivery_order(&pool, u).await;
    let mut tx = pool.begin().await.unwrap();
    let t = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ledger_transactions (id, kind, order_id, actor_id, idempotency_key)
         VALUES ($1, 'fee_adjustment', $2, $3, $1::text)",
    )
    .bind(t)
    .bind(o)
    .bind(u)
    .execute(&mut *tx)
    .await
    .unwrap();
    for (acct, ref_id, amt) in [
        ("receivable", Some(o), 10),
        ("delivery_fee_income", None, -10),
    ] {
        sqlx::query(
            "INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
             VALUES ($1, $2, $3, $4, $5::numeric)",
        )
        .bind(Uuid::now_v7())
        .bind(t)
        .bind(acct)
        .bind(ref_id)
        .bind(amt)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

#[sqlx::test]
async fn stock_movement_source_type_delivery_allowed(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = common::seed_product(&pool, u, "Widget").await;
    let r = sqlx::query(
        "INSERT INTO stock_movements (id, product_id, location_id, movement_type, quantity, source_type,
                                      source_id, reason, created_by)
         VALUES ($1, $2, '00000000-0000-7000-8002-000000000001', 'damage', -1, 'delivery', $1, 'Broken', $3)",
    )
    .bind(Uuid::now_v7())
    .bind(p)
    .bind(u)
    .execute(&pool)
    .await;
    assert!(r.is_ok(), "{r:?}");
}
