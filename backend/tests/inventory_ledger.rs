//! Ledger core (spec §2 rule 1): the only writer of stock_movements and stock_balances.
mod common;
use common::{seed_product, seed_user};
use jj_retail_api::{
    error::AppError,
    inventory::{
        domain::{MovementType, SourceType},
        ledger::{MovementInput, Source, apply},
    },
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

fn mv(product_id: Uuid, t: MovementType, quantity: i64, cost: Option<&str>) -> MovementInput {
    MovementInput {
        product_id,
        movement_type: t,
        quantity,
        unit_cost: cost.map(|c| Decimal::from_str(c).unwrap()),
        reason: (!t.is_inbound() || t == MovementType::AdjustmentIn).then(|| "Test".to_string()),
    }
}

fn src() -> Source {
    Source {
        source_type: SourceType::Adjustment,
        source_id: Uuid::now_v7(),
    }
}

async fn run(
    pool: &PgPool,
    user: Uuid,
    moves: &[MovementInput],
) -> Result<Vec<jj_retail_api::inventory::ledger::BalanceAfter>, AppError> {
    let mut tx = pool.begin().await.unwrap();
    let r = apply(&mut tx, user, src(), moves).await;
    if r.is_ok() {
        tx.commit().await.unwrap();
    }
    r
}

async fn balance(pool: &PgPool, p: Uuid) -> (i64, Option<Decimal>) {
    sqlx::query_as("SELECT on_hand, avg_cost FROM stock_balances WHERE product_id = $1")
        .bind(p)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn movement_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM stock_movements")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn setup(pool: &PgPool) -> (Uuid, Uuid) {
    let u = seed_user(pool, "admin", "correct horse battery", &["admin"]).await;
    let p = seed_product(pool, u, "Plate").await;
    (u, p)
}

#[sqlx::test]
async fn inbound_creates_balance_and_movement(pool: PgPool) {
    let (u, p) = setup(&pool).await;
    let r = run(
        &pool,
        u,
        &[mv(p, MovementType::OpeningBalance, 5, Some("100"))],
    )
    .await
    .unwrap();
    assert_eq!(r[0].on_hand, 5);
    assert_eq!(
        balance(&pool, p).await,
        (5, Some(Decimal::from_str("100.0000").unwrap()))
    );
    let q: i64 = sqlx::query_scalar("SELECT quantity FROM stock_movements WHERE product_id = $1")
        .bind(p)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(q, 5);
}

#[sqlx::test]
async fn outbound_stores_negative_quantity(pool: PgPool) {
    let (u, p) = setup(&pool).await;
    run(
        &pool,
        u,
        &[mv(p, MovementType::OpeningBalance, 5, Some("100"))],
    )
    .await
    .unwrap();
    let r = run(&pool, u, &[mv(p, MovementType::AdjustmentOut, 2, None)])
        .await
        .unwrap();
    assert_eq!(r[0].on_hand, 3);
    assert_eq!(r[0].avg_cost, Some(Decimal::from_str("100").unwrap()));
    let q: i64 = sqlx::query_scalar(
        "SELECT quantity FROM stock_movements WHERE movement_type = 'adjustment_out'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(q, -2);
}

#[sqlx::test]
async fn outbound_below_zero_is_refused_and_writes_nothing(pool: PgPool) {
    let (u, p) = setup(&pool).await;
    run(&pool, u, &[mv(p, MovementType::OpeningBalance, 3, None)])
        .await
        .unwrap();
    let e = run(&pool, u, &[mv(p, MovementType::Damage, 4, None)])
        .await
        .unwrap_err();
    assert_eq!(e.code(), "INSUFFICIENT_STOCK");
    assert!(e.to_string().contains("Only 3 of Plate"), "{e}");
    assert_eq!(movement_count(&pool).await, 1);
    assert_eq!(balance(&pool, p).await.0, 3);
}

#[sqlx::test]
async fn several_products_in_one_call(pool: PgPool) {
    let (u, a) = setup(&pool).await;
    let b = seed_product(&pool, u, "Bowl").await;
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let r = run(
        &pool,
        u,
        &[
            mv(hi, MovementType::OpeningBalance, 2, Some("10")),
            mv(lo, MovementType::OpeningBalance, 7, Some("20")),
        ],
    )
    .await
    .unwrap();
    assert_eq!(r.iter().map(|b| b.product_id).collect::<Vec<_>>(), [lo, hi]);
    assert_eq!(balance(&pool, lo).await.0, 7);
    assert_eq!(balance(&pool, hi).await.0, 2);
}

#[sqlx::test]
async fn concurrent_outbound_never_oversells(pool: PgPool) {
    let (u, p) = setup(&pool).await;
    run(&pool, u, &[mv(p, MovementType::OpeningBalance, 1, None)])
        .await
        .unwrap();
    let go = || {
        let pool = pool.clone();
        tokio::spawn(
            async move { run(&pool, u, &[mv(p, MovementType::AdjustmentOut, 1, None)]).await },
        )
    };
    let (x, y) = tokio::join!(go(), go());
    let results = [x.unwrap(), y.unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let err = results.iter().find_map(|r| r.as_ref().err()).unwrap();
    assert_eq!(err.code(), "INSUFFICIENT_STOCK");
    assert_eq!(balance(&pool, p).await.0, 0);
}
