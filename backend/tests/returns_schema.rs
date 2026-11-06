//! Returns schema invariants (Phase 5b): amounts add up, refund method iff refunded,
//! immutable rows, undo stock movements.
mod common;
use common::{seed_product, seed_user};
use sqlx::PgPool;
use uuid::Uuid;

const SHOP: &str = "00000000-0000-7000-8002-000000000001";

async fn seed_order(pool: &PgPool, u: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO orders (id, number, status, subtotal, total, receipt_token, created_by)
         VALUES ($1, $2, 'confirmed', 100, 100, $2, $3)",
    )
    .bind(id)
    .bind(id.to_string())
    .bind(u)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_return(
    pool: &PgPool,
    order: Uuid,
    u: Uuid,
    (amount, owed, refunded, method): (&str, &str, &str, Option<&str>),
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO returns (id, order_id, refund_method, amount, owed_reduced, refunded, created_by)
         VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6::numeric, $7)",
    )
    .bind(id)
    .bind(order)
    .bind(method)
    .bind(amount)
    .bind(owed)
    .bind(refunded)
    .bind(u)
    .execute(pool)
    .await?;
    Ok(id)
}

#[sqlx::test]
async fn return_amounts_must_add_up(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_order(&pool, u).await;
    assert!(
        insert_return(&pool, o, u, ("20", "10", "5", Some("cash")))
            .await
            .is_err()
    );
    assert!(
        insert_return(&pool, o, u, ("20", "15", "5", Some("cash")))
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn refund_method_iff_refunded(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let o = seed_order(&pool, u).await;
    assert!(
        insert_return(&pool, o, u, ("5", "0", "5", None))
            .await
            .is_err()
    );
    assert!(
        insert_return(&pool, o, u, ("5", "5", "0", Some("cash")))
            .await
            .is_err()
    );
    assert!(
        insert_return(&pool, o, u, ("5", "5", "0", None))
            .await
            .is_ok()
    );
    assert!(
        insert_return(&pool, o, u, ("5", "0", "5", Some("upi")))
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn return_rows_cannot_be_changed(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    let o = seed_order(&pool, u).await;
    let line = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO order_lines (id, order_id, line_no, product_id, product_name, quantity,
                                  list_price, unit_price, net_amount)
         VALUES ($1, $2, 1, $3, 'Plate', 2, 50, 50, 100)",
    )
    .bind(line)
    .bind(o)
    .bind(p)
    .execute(&pool)
    .await
    .unwrap();
    let r = insert_return(&pool, o, u, ("50", "50", "0", None))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO return_lines (id, return_id, order_line_id, quantity, disposition, amount)
         VALUES ($1, $2, $3, 1, 'sellable', 50)",
    )
    .bind(Uuid::now_v7())
    .bind(r)
    .bind(line)
    .execute(&pool)
    .await
    .unwrap();
    for sql in [
        "UPDATE return_lines SET quantity = 2",
        "DELETE FROM return_lines",
        "UPDATE returns SET note = 'x'",
        "DELETE FROM returns",
    ] {
        let e = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(e.to_string().contains("cannot be changed"), "{sql}: {e}");
    }
}

#[sqlx::test]
async fn sale_return_movement_is_inbound(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    let ins = |qty: i64, mt: &'static str, source: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO stock_movements (id, product_id, location_id, quantity, movement_type,
                                              source_type, source_id, created_by)
                 VALUES ($1, $2, $3::uuid, $4, $5, $6, $7, $8)",
            )
            .bind(Uuid::now_v7())
            .bind(p)
            .bind(SHOP)
            .bind(qty)
            .bind(mt)
            .bind(source)
            .bind(Uuid::now_v7())
            .bind(u)
            .execute(&pool)
            .await
        }
    };
    assert!(ins(-1, "sale_return", "return").await.is_err());
    assert!(ins(1, "sale_return", "return").await.is_ok());
    assert!(ins(-1, "sale_cancel", "order").await.is_err());
    assert!(ins(1, "sale_cancel", "order").await.is_ok());
}
