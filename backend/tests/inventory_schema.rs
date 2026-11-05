//! Inventory schema invariants (spec §1): seeded location, immutable ledger, sign and floor checks.
mod common;
use common::{seed_product, seed_user};
use sqlx::PgPool;
use uuid::Uuid;

const SHOP: &str = "00000000-0000-7000-8002-000000000001";

async fn insert_movement(
    pool: &PgPool,
    product: Uuid,
    user: Uuid,
    ty: &str,
    qty: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO stock_movements (id, product_id, location_id, quantity, movement_type, source_type, source_id, created_by)
         VALUES ($1, $2, $3::uuid, $4, $5, 'stock_count', $6, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(product)
    .bind(SHOP)
    .bind(qty)
    .bind(ty)
    .bind(Uuid::now_v7())
    .bind(user)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test]
async fn seeds_shop_location(pool: PgPool) {
    let row: (String, String) =
        sqlx::query_as("SELECT code, name FROM locations WHERE id = $1::uuid")
            .bind(SHOP)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, ("shop".to_string(), "Shop".to_string()));
}

#[sqlx::test]
async fn movements_cannot_be_updated_or_deleted(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    insert_movement(&pool, p, u, "opening_balance", 5)
        .await
        .unwrap();
    let e = sqlx::query("UPDATE stock_movements SET quantity = 6")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("cannot be changed"), "{e}");
    let e = sqlx::query("DELETE FROM stock_movements")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("cannot be changed"), "{e}");
}

#[sqlx::test]
async fn movement_sign_must_match_type(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    assert!(
        insert_movement(&pool, p, u, "purchase_receipt", -1)
            .await
            .is_err()
    );
    assert!(insert_movement(&pool, p, u, "damage", 1).await.is_err());
    assert!(
        insert_movement(&pool, p, u, "adjustment_out", -1)
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn balance_cannot_go_negative(pool: PgPool) {
    let u = seed_user(&pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(&pool, u, "Plate").await;
    let r = sqlx::query(
        "INSERT INTO stock_balances (product_id, location_id, on_hand) VALUES ($1, $2::uuid, -1)",
    )
    .bind(p)
    .bind(SHOP)
    .execute(&pool)
    .await;
    assert!(r.is_err());
}

#[sqlx::test]
async fn active_supplier_names_are_unique_case_insensitive(pool: PgPool) {
    let ins = |name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query("INSERT INTO suppliers (id, name) VALUES ($1, $2)")
                .bind(Uuid::now_v7())
                .bind(name)
                .execute(&pool)
                .await
        }
    };
    ins("Superware").await.unwrap();
    assert!(ins("superware").await.is_err());
    sqlx::query("UPDATE suppliers SET archived_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    ins("superware").await.unwrap();
}
