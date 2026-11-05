//! Inventory: stock ledger, suppliers, goods receipts, stock counts (Architecture.md §8, §9).
pub mod domain;
pub mod ledger;
pub mod receipts;
pub mod suppliers;

use uuid::Uuid;

/// The one seeded location; the screens never show it.
pub const SHOP_LOCATION_ID: Uuid = Uuid::from_u128(0x00000000_0000_7000_8002_000000000001);

pub fn router() -> axum::Router<crate::AppState> {
    axum::Router::new()
        .merge(suppliers::router())
        .merge(receipts::router())
}

/// India Standard Time is a fixed UTC+05:30 with no daylight saving.
pub fn ist() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(19_800).expect("valid offset")
}

/// Today's date at the shop.
pub fn today_ist() -> chrono::NaiveDate {
    chrono::Utc::now().with_timezone(&ist()).date_naive()
}

pub(crate) fn line_field(
    i: usize,
    key: &str,
    message: impl Into<String>,
) -> crate::error::AppError {
    crate::error::AppError::FieldInvalid {
        field: format!("lines[{i}].{key}"),
        message: message.into(),
    }
}

/// Every line product must exist, be active and appear once. `doc` is "receipt" or "count".
pub(crate) async fn check_line_products(
    conn: &mut sqlx::PgConnection,
    ids: &[Uuid],
    doc: &str,
) -> crate::error::AppResult<()> {
    let rows = sqlx::query!(
        r#"SELECT p.id, p.archived_at IS NOT NULL AS "archived!",
                  COALESCE((SELECT raw_value FROM product_identifiers
                             WHERE product_id = p.id AND identifier_type = 'product_code'),
                           p.name) AS "label!"
           FROM products p WHERE p.id = ANY($1)"#,
        ids
    )
    .fetch_all(&mut *conn)
    .await?;
    let found: std::collections::HashMap<Uuid, (bool, String)> = rows
        .into_iter()
        .map(|r| (r.id, (r.archived, r.label)))
        .collect();
    let mut seen = std::collections::HashSet::new();
    for (i, id) in ids.iter().enumerate() {
        let Some((archived, label)) = found.get(id) else {
            return Err(line_field(i, "product_id", "Pick a product from the list."));
        };
        if *archived {
            return Err(line_field(
                i,
                "product_id",
                format!("{label} is archived. Remove it from this {doc}."),
            ));
        }
        if !seen.insert(*id) {
            return Err(line_field(
                i,
                "product_id",
                format!("{label} is already on this {doc}. Change that line instead."),
            ));
        }
    }
    Ok(())
}
