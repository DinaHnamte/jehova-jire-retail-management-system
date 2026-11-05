//! Sales and money schema invariants (Phase 5 spec §1): balanced ledger, immutability,
//! phone and total checks, sale stock movements, seeded shop name.
mod common;
use common::{seed_product, seed_user};
use sqlx::PgPool;
use uuid::Uuid;

const SHOP: &str = "00000000-0000-7000-8002-000000000001";

async fn insert_txn(pool: &PgPool, actor: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ledger_transactions (id, kind, method, actor_id, idempotency_key)
         VALUES ($1, 'sale', 'internal', $2, $3)",
    )
    .bind(id)
    .bind(actor)
    .bind(id.to_string())
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Inserts a transaction with the given entries inside one DB transaction and commits it.
async fn post(
    pool: &PgPool,
    actor: Uuid,
    entries: &[(&str, Option<Uuid>, &str)],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ledger_transactions (id, kind, method, actor_id, idempotency_key)
         VALUES ($1, 'sale', 'internal', $2, $3)",
    )
    .bind(id)
    .bind(actor)
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    for (account, ref_id, amount) in entries {
        sqlx::query(
            "INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
             VALUES ($1, $2, $3, $4, $5::numeric)",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(account)
        .bind(ref_id)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

#[sqlx::test]
async fn unbalanced_ledger_transaction_is_rejected_at_commit(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let e = post(
        &pool,
        u,
        &[
            ("receivable", Some(Uuid::now_v7()), "100"),
            ("sales_revenue", None, "-90"),
        ],
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("does not balance"), "{e}");
}

#[sqlx::test]
async fn balanced_ledger_transaction_commits(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    post(
        &pool,
        u,
        &[
            ("receivable", Some(Uuid::now_v7()), "100"),
            ("sales_revenue", None, "-100"),
        ],
    )
    .await
    .unwrap();
}

#[sqlx::test]
async fn ledger_rows_cannot_be_changed(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    post(
        &pool,
        u,
        &[("cash", None, "100"), ("sales_revenue", None, "-100")],
    )
    .await
    .unwrap();
    for sql in [
        "UPDATE ledger_entries SET amount = 5",
        "DELETE FROM ledger_entries",
        "UPDATE ledger_transactions SET reason = 'x'",
        "DELETE FROM ledger_transactions",
    ] {
        let e = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(e.to_string().contains("cannot be changed"), "{sql}: {e}");
    }
}

#[sqlx::test]
async fn receivable_needs_an_order_ref(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let txn = insert_txn(&pool, u).await;
    let r = sqlx::query(
        "INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
         VALUES ($1, $2, 'receivable', NULL, 100)",
    )
    .bind(Uuid::now_v7())
    .bind(txn)
    .execute(&pool)
    .await;
    assert!(r.is_err());
}

#[sqlx::test]
async fn customer_phone_must_be_ten_digits(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let ins = |phone: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query("INSERT INTO customers (id, phone, created_by) VALUES ($1, $2, $3)")
                .bind(Uuid::now_v7())
                .bind(phone)
                .bind(u)
                .execute(&pool)
                .await
        }
    };
    assert!(ins("98620").await.is_err());
    assert!(ins("9862012345").await.is_ok());
    assert!(ins("9862012345").await.is_err());
}

#[sqlx::test]
async fn order_total_must_match(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let ins = |total: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO orders (id, number, status, subtotal, order_discount, total,
                                     receipt_token, created_by)
                 VALUES ($1, $2, 'confirmed', 100, 10, $3::numeric, $2, $4)",
            )
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7().to_string())
            .bind(total)
            .bind(u)
            .execute(&pool)
            .await
        }
    };
    assert!(ins("95").await.is_err());
    assert!(ins("90").await.is_ok());
}

#[sqlx::test]
async fn sale_movement_must_be_negative(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    let ins = |qty: i64, source: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO stock_movements (id, product_id, location_id, quantity, movement_type,
                                              source_type, source_id, created_by)
                 VALUES ($1, $2, $3::uuid, $4, 'sale', $5, $6, $7)",
            )
            .bind(Uuid::now_v7())
            .bind(p)
            .bind(SHOP)
            .bind(qty)
            .bind(source)
            .bind(Uuid::now_v7())
            .bind(u)
            .execute(&pool)
            .await
        }
    };
    assert!(ins(1, "order").await.is_err());
    assert!(ins(-1, "order").await.is_ok());
}

#[sqlx::test]
async fn shop_name_is_seeded(pool: PgPool) {
    let name: String =
        sqlx::query_scalar("SELECT value #>> '{}' FROM settings WHERE key = 'shop_name'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name, "Jehovah Jireh");
}
