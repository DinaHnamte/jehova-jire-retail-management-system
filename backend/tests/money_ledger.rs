//! Money ledger writer: balanced posting, owed balances, open orders (Phase 5a Task 3).
mod common;
use chrono::{DateTime, Duration, Utc};
use common::seed_user;
use jj_retail_api::error::AppError;
use jj_retail_api::ledger::{
    Account, Method, TxnInput, TxnKind, customer_open_orders, customer_owes, order_owed,
    orders_owed, post,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

async fn seed_customer(pool: &PgPool, actor: Uuid, phone: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO customers (id, phone, created_by) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(phone)
        .bind(actor)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn seed_order(
    pool: &PgPool,
    actor: Uuid,
    customer: Option<Uuid>,
    total: &str,
    created_at: DateTime<Utc>,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO orders (id, number, customer_id, status, subtotal, total, receipt_token, created_by, created_at)
         VALUES ($1, $2, $3, 'confirmed', $4::numeric, $4::numeric, $5, $6, $7)",
    )
    .bind(id)
    .bind(id.to_string())
    .bind(customer)
    .bind(total)
    .bind(id.to_string())
    .bind(actor)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn input(
    kind: TxnKind,
    method: Method,
    actor: Uuid,
    order: Uuid,
    customer: Option<Uuid>,
    suffix: &str,
    entries: Vec<(Account, Decimal)>,
) -> TxnInput<'static> {
    TxnInput {
        kind,
        order_id: Some(order),
        customer_id: customer,
        method,
        actor,
        idempotency_key: format!("{order}:{suffix}"),
        note: None,
        entries,
    }
}

async fn sell(pool: &PgPool, actor: Uuid, customer: Option<Uuid>, order: Uuid, amount: &str) {
    let mut tx = pool.begin().await.unwrap();
    post(
        &mut tx,
        input(
            TxnKind::Sale,
            Method::Internal,
            actor,
            order,
            customer,
            "sale",
            vec![
                (Account::Receivable(order), d(amount)),
                (Account::SalesRevenue, -d(amount)),
            ],
        ),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test]
async fn post_writes_entries_and_owed_is_their_sum(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_order(&pool, u, None, "1000", Utc::now()).await;
    sell(&pool, u, None, o, "1000").await;
    let mut tx = pool.begin().await.unwrap();
    post(
        &mut tx,
        input(
            TxnKind::Payment,
            Method::Cash,
            u,
            o,
            None,
            "payment",
            vec![
                (Account::Cash, d("600")),
                (Account::Receivable(o), d("-600")),
            ],
        ),
    )
    .await
    .unwrap();
    assert_eq!(order_owed(&mut tx, o).await.unwrap(), d("400.00"));
    assert_eq!(orders_owed(&mut tx, &[o]).await.unwrap()[&o], d("400.00"));
    tx.commit().await.unwrap();

    let received: Option<Uuid> =
        sqlx::query_scalar("SELECT received_by FROM ledger_transactions WHERE kind = 'payment'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(received, Some(u));
    let received: Option<Uuid> =
        sqlx::query_scalar("SELECT received_by FROM ledger_transactions WHERE kind = 'sale'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(received, None);
}

#[sqlx::test]
async fn post_refuses_unbalanced_entries_in_code(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_order(&pool, u, None, "100", Utc::now()).await;
    let mut tx = pool.begin().await.unwrap();
    let err = post(
        &mut tx,
        input(
            TxnKind::Sale,
            Method::Internal,
            u,
            o,
            None,
            "sale",
            vec![
                (Account::Receivable(o), d("100")),
                (Account::SalesRevenue, d("-90")),
            ],
        ),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Internal(_)));
    let empty = post(
        &mut tx,
        input(TxnKind::Sale, Method::Internal, u, o, None, "x", vec![]),
    )
    .await;
    assert!(matches!(empty, Err(AppError::Internal(_))));
    let zero = post(
        &mut tx,
        input(
            TxnKind::Sale,
            Method::Internal,
            u,
            o,
            None,
            "y",
            vec![(Account::Cash, d("0")), (Account::SalesRevenue, d("0"))],
        ),
    )
    .await;
    assert!(matches!(zero, Err(AppError::Internal(_))));
    tx.commit().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn customer_open_orders_is_oldest_first_and_skips_paid(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let c = seed_customer(&pool, u, "9876543210").await;
    let now = Utc::now();
    let newest = seed_order(&pool, u, Some(c), "200", now).await;
    let paid = seed_order(&pool, u, Some(c), "500", now - Duration::hours(1)).await;
    let oldest = seed_order(&pool, u, Some(c), "300", now - Duration::hours(2)).await;
    for (o, amt) in [(newest, "200"), (paid, "500"), (oldest, "300")] {
        sell(&pool, u, Some(c), o, amt).await;
    }
    let mut tx = pool.begin().await.unwrap();
    post(
        &mut tx,
        input(
            TxnKind::Payment,
            Method::Upi,
            u,
            paid,
            Some(c),
            "payment",
            vec![
                (Account::Upi, d("500")),
                (Account::Receivable(paid), d("-500")),
            ],
        ),
    )
    .await
    .unwrap();
    let open = customer_open_orders(&mut tx, c).await.unwrap();
    assert_eq!(open, vec![(oldest, d("300.00")), (newest, d("200.00"))]);
    assert_eq!(customer_owes(&mut tx, c).await.unwrap(), d("500.00"));
    tx.commit().await.unwrap();
}
