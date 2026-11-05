//! SQL for the catalogue. Every function takes the caller's connection/transaction.
use sqlx::PgConnection;
use uuid::Uuid;

use super::dto::NamedRef;
use crate::error::AppResult;

pub async fn list_brands(conn: &mut PgConnection) -> AppResult<Vec<NamedRef>> {
    Ok(
        sqlx::query_as!(NamedRef, "SELECT id, name FROM brands ORDER BY lower(name)")
            .fetch_all(conn)
            .await?,
    )
}

pub async fn insert_brand(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO brands (id, name) VALUES ($1, $2)", id, name)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn list_categories(conn: &mut PgConnection) -> AppResult<Vec<NamedRef>> {
    Ok(sqlx::query_as!(
        NamedRef,
        "SELECT id, name FROM categories ORDER BY lower(name)"
    )
    .fetch_all(conn)
    .await?)
}

pub async fn insert_category(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO categories (id, name) VALUES ($1, $2)",
        id,
        name
    )
    .execute(conn)
    .await?;
    Ok(())
}

// ---------- products ----------

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use super::{
    domain::{ProductDraft, ValidProduct, variant_num},
    dto::IdentifierOut,
};
use crate::shared::code::ProductCode;

/// One product with its brand, category and product code.
pub struct ProductRow {
    pub id: Uuid,
    pub brand_id: Uuid,
    pub brand_name: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub unit: String,
    pub pack_qty: i32,
    pub attributes: Value,
    pub selling_price: Decimal,
    pub mrp: Option<Decimal>,
    pub default_cost: Option<Decimal>,
    pub hsn_code: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub code_raw: Option<String>,
    pub code: Option<String>,
    pub base_code: Option<String>,
    pub variant: Option<String>,
    pub design_code: Option<String>,
}

impl ProductRow {
    /// The stored values as a draft, so a PATCH can overlay changes and re-validate.
    pub fn to_draft(&self) -> ProductDraft {
        ProductDraft {
            name: self.name.clone(),
            code: self.code_raw.clone().unwrap_or_default(),
            category_id: self.category_id,
            description: self.description.clone(),
            unit: Some(self.unit.clone()),
            pack_qty: Some(self.pack_qty),
            attributes: Some(self.attributes.clone()),
            selling_price: Some(self.selling_price.to_string()),
            mrp: self.mrp.map(|d| d.to_string()),
            default_cost: self.default_cost.map(|d| d.to_string()),
            hsn_code: self.hsn_code.clone(),
            design_code: self.design_code.clone(),
        }
    }
}

pub async fn get_product(
    conn: &mut PgConnection,
    id: Uuid,
    for_update: bool,
) -> AppResult<Option<ProductRow>> {
    // Two literal queries so both are checked at compile time.
    let row = if for_update {
        sqlx::query_as!(
            ProductRow,
            r#"SELECT p.id, p.brand_id, b.name AS brand_name, p.category_id, c.name AS "category_name?",
                      p.name, p.description, p.unit, p.pack_qty, p.attributes, p.selling_price, p.mrp,
                      p.default_cost, p.hsn_code, p.archived_at, p.created_at, p.updated_at,
                      pi.raw_value AS "code_raw?", pi.normalized_value AS "code?",
                      pi.base_code AS "base_code?", pi.variant AS "variant?", pi.design_code AS "design_code?"
               FROM products p
               JOIN brands b ON b.id = p.brand_id
               LEFT JOIN categories c ON c.id = p.category_id
               LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
               WHERE p.id = $1
               FOR UPDATE OF p"#,
            id
        )
        .fetch_optional(conn)
        .await?
    } else {
        sqlx::query_as!(
            ProductRow,
            r#"SELECT p.id, p.brand_id, b.name AS brand_name, p.category_id, c.name AS "category_name?",
                      p.name, p.description, p.unit, p.pack_qty, p.attributes, p.selling_price, p.mrp,
                      p.default_cost, p.hsn_code, p.archived_at, p.created_at, p.updated_at,
                      pi.raw_value AS "code_raw?", pi.normalized_value AS "code?",
                      pi.base_code AS "base_code?", pi.variant AS "variant?", pi.design_code AS "design_code?"
               FROM products p
               JOIN brands b ON b.id = p.brand_id
               LEFT JOIN categories c ON c.id = p.category_id
               LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
               WHERE p.id = $1"#,
            id
        )
        .fetch_optional(conn)
        .await?
    };
    Ok(row)
}

pub async fn list_identifiers(
    conn: &mut PgConnection,
    product_id: Uuid,
) -> AppResult<Vec<IdentifierOut>> {
    Ok(sqlx::query_as!(
        IdentifierOut,
        "SELECT id, identifier_type, raw_value, normalized_value FROM product_identifiers
         WHERE product_id = $1
         ORDER BY identifier_type = 'product_code' DESC, created_at, id",
        product_id
    )
    .fetch_all(conn)
    .await?)
}

pub async fn brand_exists(conn: &mut PgConnection, id: Uuid) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar!("SELECT EXISTS (SELECT 1 FROM brands WHERE id = $1)", id)
            .fetch_one(conn)
            .await?
            .unwrap_or(false),
    )
}

pub async fn category_exists(conn: &mut PgConnection, id: Uuid) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar!("SELECT EXISTS (SELECT 1 FROM categories WHERE id = $1)", id)
            .fetch_one(conn)
            .await?
            .unwrap_or(false),
    )
}

pub async fn insert_product(
    conn: &mut PgConnection,
    id: Uuid,
    brand_id: Uuid,
    p: &ValidProduct,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO products (id, brand_id, category_id, name, description, unit, pack_qty, attributes,
                               selling_price, mrp, default_cost, hsn_code, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        id,
        brand_id,
        p.category_id,
        p.name,
        p.description,
        p.unit,
        p.pack_qty,
        p.attributes,
        p.selling_price,
        p.mrp,
        p.default_cost,
        p.hsn_code,
        created_by
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_product(
    conn: &mut PgConnection,
    id: Uuid,
    p: &ValidProduct,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE products SET category_id = $2, name = $3, description = $4, unit = $5, pack_qty = $6,
                attributes = $7, selling_price = $8, mrp = $9, default_cost = $10, hsn_code = $11,
                updated_at = now()
         WHERE id = $1",
        id,
        p.category_id,
        p.name,
        p.description,
        p.unit,
        p.pack_qty,
        p.attributes,
        p.selling_price,
        p.mrp,
        p.default_cost,
        p.hsn_code
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert the product's code, or replace it if the product already has one.
pub async fn upsert_code(
    conn: &mut PgConnection,
    product_id: Uuid,
    brand_id: Uuid,
    raw: &str,
    code: &ProductCode,
    design: Option<&str>,
) -> Result<(), sqlx::Error> {
    let num = variant_num(code.variant.as_deref());
    let updated = sqlx::query!(
        "UPDATE product_identifiers
         SET raw_value = $3, normalized_value = $4, base_code = $5, variant = $6, variant_num = $7, digits = $8,
             design_code = $9
         WHERE product_id = $1 AND brand_id = $2 AND identifier_type = 'product_code'",
        product_id,
        brand_id,
        raw,
        code.normalized,
        code.base,
        code.variant,
        num,
        code.digits,
        design
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if updated == 0 {
        sqlx::query!(
            "INSERT INTO product_identifiers
               (id, product_id, brand_id, identifier_type, raw_value, normalized_value, base_code, variant, variant_num,
                digits, design_code)
             VALUES ($1, $2, $3, 'product_code', $4, $5, $6, $7, $8, $9, $10)",
            crate::shared::ids::new_id(),
            product_id,
            brand_id,
            raw,
            code.normalized,
            code.base,
            code.variant,
            num,
            code.digits,
            design
        )
        .execute(conn)
        .await?;
    }
    Ok(())
}

pub async fn insert_identifier(
    conn: &mut PgConnection,
    id: Uuid,
    product_id: Uuid,
    brand_id: Uuid,
    identifier_type: &str,
    raw: &str,
    normalized: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO product_identifiers (id, product_id, brand_id, identifier_type, raw_value, normalized_value)
         VALUES ($1, $2, $3, $4, $5, $6)",
        id,
        product_id,
        brand_id,
        identifier_type,
        raw,
        normalized
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn identifier_type_of(
    conn: &mut PgConnection,
    product_id: Uuid,
    id: Uuid,
) -> AppResult<Option<(String, String)>> {
    Ok(sqlx::query!(
        "SELECT identifier_type, raw_value FROM product_identifiers WHERE id = $1 AND product_id = $2",
        id,
        product_id
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.identifier_type, r.raw_value)))
}

pub async fn delete_identifier(conn: &mut PgConnection, id: Uuid) -> AppResult<()> {
    sqlx::query!("DELETE FROM product_identifiers WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Who already holds this identifier: `(product_id, name, archived)`.
pub async fn identifier_owner(
    conn: &mut PgConnection,
    brand_id: Uuid,
    identifier_type: &str,
    normalized: &str,
    design: Option<&str>,
) -> AppResult<Option<(Uuid, String, bool)>> {
    Ok(sqlx::query!(
        "SELECT p.id, p.name, p.archived_at IS NOT NULL AS \"archived!\"
         FROM product_identifiers pi JOIN products p ON p.id = pi.product_id
         WHERE pi.brand_id = $1 AND pi.identifier_type = $2 AND pi.normalized_value = $3
           AND pi.design_code IS NOT DISTINCT FROM $4",
        brand_id,
        identifier_type,
        normalized,
        design
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.id, r.name, r.archived)))
}

/// Set or clear `archived_at`. Returns whether anything changed.
pub async fn set_archived(conn: &mut PgConnection, id: Uuid, archived: bool) -> AppResult<bool> {
    let n = if archived {
        sqlx::query!(
            "UPDATE products SET archived_at = now(), updated_at = now() WHERE id = $1 AND archived_at IS NULL",
            id
        )
        .execute(conn)
        .await?
        .rows_affected()
    } else {
        sqlx::query!(
            "UPDATE products SET archived_at = NULL, updated_at = now() WHERE id = $1 AND archived_at IS NOT NULL",
            id
        )
        .execute(conn)
        .await?
        .rows_affected()
    };
    Ok(n > 0)
}

/// Every identifier as `(brand_id, type, normalized_value, design_code)`, for import
/// duplicate checks.
pub async fn all_identifier_keys(conn: &mut PgConnection) -> AppResult<Vec<IdentityKey>> {
    Ok(sqlx::query!(
        "SELECT brand_id, identifier_type, normalized_value, design_code FROM product_identifiers"
    )
    .fetch_all(conn)
    .await?
    .into_iter()
    .map(|r| {
        (
            r.brand_id,
            r.identifier_type,
            r.normalized_value,
            r.design_code,
        )
    })
    .collect())
}

/// What makes an identifier unique: brand, type, normalised value and design.
pub type IdentityKey = (Uuid, String, String, Option<String>);
