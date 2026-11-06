//! Double-entry money ledger writer and balance queries (Architecture.md §14).
//! Debit is positive, credit negative. The database re-checks that every transaction sums to
//! zero at commit; `post` checks first so a bug fails fast with a clear error.

use crate::error::{AppError, AppResult};
use crate::shared::ids::new_id;
use anyhow::anyhow;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

pub enum Account {
    Receivable(Uuid),
    Cash,
    Upi,
    SalesRevenue,
    SalesReturns,
}

impl Account {
    fn parts(&self) -> (&'static str, Option<Uuid>) {
        match self {
            Self::Receivable(order) => ("receivable", Some(*order)),
            Self::Cash => ("cash", None),
            Self::Upi => ("upi", None),
            Self::SalesRevenue => ("sales_revenue", None),
            Self::SalesReturns => ("sales_returns", None),
        }
    }
}

pub enum TxnKind {
    Sale,
    Payment,
    /// Reverses a sale (`reverses_id` names it).
    Cancel,
    /// Money handed back in cash or UPI.
    Refund,
    /// A return that reduces what is owed.
    ReturnCredit,
}

impl TxnKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::Payment => "payment",
            Self::Cancel => "cancel",
            Self::Refund => "refund",
            Self::ReturnCredit => "return_credit",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Cash,
    Upi,
    Internal,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Upi => "upi",
            Self::Internal => "internal",
        }
    }
}

pub struct TxnInput<'a> {
    pub kind: TxnKind,
    pub order_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub method: Method,
    pub actor: Uuid,
    /// Request key plus a suffix (`:sale`, `:payment`), unique per transaction.
    pub idempotency_key: String,
    pub note: Option<&'a str>,
    /// The transaction this one reverses (a cancel names its sale).
    pub reverses_id: Option<Uuid>,
    pub entries: Vec<(Account, Decimal)>,
}

/// Writes one balanced ledger transaction inside the caller's DB transaction.
/// An empty, zero-amount or unbalanced entry list is a programming error (`Internal`).
pub async fn post(conn: &mut PgConnection, t: TxnInput<'_>) -> AppResult<Uuid> {
    if t.entries.is_empty() {
        return Err(AppError::Internal(anyhow!(
            "ledger transaction has no entries"
        )));
    }
    if t.entries.iter().any(|(_, amount)| amount.is_zero()) {
        return Err(AppError::Internal(anyhow!(
            "ledger entry has a zero amount"
        )));
    }
    let sum: Decimal = t.entries.iter().map(|(_, amount)| *amount).sum();
    if !sum.is_zero() {
        return Err(AppError::Internal(anyhow!(
            "ledger transaction does not balance: {sum}"
        )));
    }

    // Cash and UPI payments are received by the actor; Internal moves nothing physical.
    let received_by = (t.method != Method::Internal).then_some(t.actor);
    let id = new_id();
    sqlx::query!(
        "INSERT INTO ledger_transactions
             (id, kind, order_id, customer_id, method, received_by, note, actor_id, idempotency_key,
              reverses_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        id,
        t.kind.as_str(),
        t.order_id,
        t.customer_id,
        t.method.as_str(),
        received_by,
        t.note,
        t.actor,
        t.idempotency_key,
        t.reverses_id,
    )
    .execute(&mut *conn)
    .await?;
    for (account, amount) in &t.entries {
        let (name, ref_id) = account.parts();
        sqlx::query!(
            "INSERT INTO ledger_entries (id, transaction_id, account, ref_id, amount)
             VALUES ($1, $2, $3, $4, $5)",
            new_id(),
            id,
            name,
            ref_id,
            amount,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

/// What is still owed on one order (0 when it has no receivable entries).
pub async fn order_owed(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Decimal> {
    Ok(orders_owed(conn, &[order_id])
        .await?
        .remove(&order_id)
        .unwrap_or(Decimal::ZERO))
}

/// What is owed on each of the given orders; orders without entries map to 0.
pub async fn orders_owed(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> AppResult<HashMap<Uuid, Decimal>> {
    let rows = sqlx::query!(
        r#"SELECT ref_id AS "ref_id!", COALESCE(SUM(amount), 0) AS "owed!"
           FROM ledger_entries
           WHERE account = 'receivable' AND ref_id = ANY($1)
           GROUP BY ref_id"#,
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut owed: HashMap<Uuid, Decimal> = ids.iter().map(|id| (*id, Decimal::ZERO)).collect();
    for r in rows {
        owed.insert(r.ref_id, r.owed);
    }
    Ok(owed)
}

/// Unpaid confirmed orders of a customer, oldest first, with what's owed on each (> 0 only).
pub async fn customer_open_orders(
    conn: &mut PgConnection,
    customer_id: Uuid,
) -> AppResult<Vec<(Uuid, Decimal)>> {
    let rows = sqlx::query!(
        r#"SELECT o.id, SUM(e.amount) AS "owed!"
           FROM orders o
           JOIN ledger_entries e ON e.account = 'receivable' AND e.ref_id = o.id
           WHERE o.customer_id = $1 AND o.status = 'confirmed'
           GROUP BY o.id, o.created_at
           HAVING SUM(e.amount) > 0
           ORDER BY o.created_at, o.id"#,
        customer_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.owed)).collect())
}

/// Everything a customer owes, across all their orders.
pub async fn customer_owes(conn: &mut PgConnection, customer_id: Uuid) -> AppResult<Decimal> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(e.amount), 0) AS "owes!"
           FROM orders o
           JOIN ledger_entries e ON e.account = 'receivable' AND e.ref_id = o.id
           WHERE o.customer_id = $1"#,
        customer_id,
    )
    .fetch_one(&mut *conn)
    .await?)
}
