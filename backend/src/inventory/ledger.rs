//! The only writer of `stock_movements` and `stock_balances` (spec §2 rule 1).
//! Callers own the transaction; balances are locked in product_id order so concurrent
//! writers can't deadlock (Architecture.md §8.3).
use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    SHOP_LOCATION_ID,
    domain::{MovementType, SourceType, next_avg_cost},
};
use crate::{
    error::{AppError, AppResult},
    shared::ids::new_id,
};

pub struct MovementInput {
    pub product_id: Uuid,
    pub movement_type: MovementType,
    /// Always positive; the sign comes from `movement_type`.
    pub quantity: i64,
    pub unit_cost: Option<Decimal>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceAfter {
    pub product_id: Uuid,
    pub on_hand: i64,
    pub avg_cost: Option<Decimal>,
}

pub struct Source {
    pub source_type: SourceType,
    pub source_id: Uuid,
}

/// Insert missing balance rows (0, unknown cost), then lock them FOR UPDATE in product_id order.
pub async fn lock_balances(
    conn: &mut PgConnection,
    product_ids: &[Uuid],
) -> AppResult<HashMap<Uuid, (i64, Option<Decimal>)>> {
    let mut ids = product_ids.to_vec();
    ids.sort();
    ids.dedup();
    sqlx::query!(
        "INSERT INTO stock_balances (product_id, location_id)
         SELECT p, $2 FROM unnest($1::uuid[]) AS p ORDER BY p
         ON CONFLICT DO NOTHING",
        &ids,
        SHOP_LOCATION_ID
    )
    .execute(&mut *conn)
    .await?;
    let rows = sqlx::query!(
        "SELECT product_id, on_hand, avg_cost FROM stock_balances
         WHERE location_id = $2 AND product_id = ANY($1)
         ORDER BY product_id FOR UPDATE",
        &ids,
        SHOP_LOCATION_ID
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.product_id, (r.on_hand, r.avg_cost)))
        .collect())
}

/// The product's code, or its name when it has none; used in messages.
pub async fn product_label(conn: &mut PgConnection, product_id: Uuid) -> AppResult<String> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COALESCE(
               (SELECT normalized_value FROM product_identifiers
                 WHERE product_id = p.id AND identifier_type = 'product_code'),
               p.name) AS "label!"
           FROM products p WHERE p.id = $1"#,
        product_id
    )
    .fetch_one(conn)
    .await?)
}

/// Write `moves` and update balances. Returns the new balances in product_id order.
/// An outbound move that would take stock below zero fails with INSUFFICIENT_STOCK;
/// the caller's transaction must then be rolled back.
pub async fn apply(
    conn: &mut PgConnection,
    actor: Uuid,
    source: Source,
    moves: &[MovementInput],
) -> AppResult<Vec<BalanceAfter>> {
    let mut order: Vec<&MovementInput> = moves.iter().collect();
    order.sort_by_key(|m| m.product_id);
    let ids: Vec<Uuid> = order.iter().map(|m| m.product_id).collect();
    let mut balances = lock_balances(conn, &ids).await?;

    for m in &order {
        if m.quantity <= 0 {
            return Err(AppError::Internal(anyhow::anyhow!(
                "movement quantity must be positive, got {}",
                m.quantity
            )));
        }
        let (on_hand, avg) = balances
            .get_mut(&m.product_id)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("balance row not locked")))?;
        let signed = if m.movement_type.is_inbound() {
            *avg = next_avg_cost(*on_hand, *avg, m.quantity, m.unit_cost);
            *on_hand += m.quantity;
            m.quantity
        } else {
            if *on_hand < m.quantity {
                let label = product_label(conn, m.product_id).await?;
                return Err(AppError::insufficient_stock(*on_hand, &label, m.product_id));
            }
            *on_hand -= m.quantity;
            -m.quantity
        };
        sqlx::query!(
            "INSERT INTO stock_movements
               (id, product_id, location_id, quantity, unit_cost, movement_type,
                source_type, source_id, reason, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            new_id(),
            m.product_id,
            SHOP_LOCATION_ID,
            signed,
            m.unit_cost,
            m.movement_type.as_str(),
            source.source_type.as_str(),
            source.source_id,
            m.reason.as_deref(),
            actor
        )
        .execute(&mut *conn)
        .await?;
    }

    let mut out: Vec<BalanceAfter> = balances
        .into_iter()
        .map(|(product_id, (on_hand, avg_cost))| BalanceAfter {
            product_id,
            on_hand,
            avg_cost,
        })
        .collect();
    out.sort_by_key(|b| b.product_id);
    for b in &out {
        sqlx::query!(
            "UPDATE stock_balances SET on_hand = $3, avg_cost = $4, updated_at = now()
             WHERE product_id = $1 AND location_id = $2",
            b.product_id,
            SHOP_LOCATION_ID,
            b.on_hand,
            b.avg_cost
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(out)
}
