//! Catalogue use cases. Each opens and owns its transaction.
use sqlx::PgPool;

use super::{dto::NamedRef, is_unique_violation, repo};
use crate::{
    error::{AppError, AppResult},
    shared::ids::new_id,
};

const MAX_NAME: usize = 100;

fn clean_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME {
        return Err(AppError::Validation(format!(
            "Enter a name of 1 to {MAX_NAME} characters."
        )));
    }
    Ok(n.to_string())
}

pub async fn list_brands(db: &PgPool) -> AppResult<Vec<NamedRef>> {
    repo::list_brands(&mut *db.acquire().await?).await
}

pub async fn create_brand(db: &PgPool, name: &str) -> AppResult<NamedRef> {
    let name = clean_name(name)?;
    let id = new_id();
    repo::insert_brand(&mut *db.acquire().await?, id, &name)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict(format!("A brand called {name} already exists."))
            } else {
                e.into()
            }
        })?;
    Ok(NamedRef { id, name })
}

pub async fn list_categories(db: &PgPool) -> AppResult<Vec<NamedRef>> {
    repo::list_categories(&mut *db.acquire().await?).await
}

pub async fn create_category(db: &PgPool, name: &str) -> AppResult<NamedRef> {
    let name = clean_name(name)?;
    let id = new_id();
    repo::insert_category(&mut *db.acquire().await?, id, &name)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict(format!("A category called {name} already exists."))
            } else {
                e.into()
            }
        })?;
    Ok(NamedRef { id, name })
}

// ---------- products ----------

use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    EKTRA_BRAND_ID,
    domain::{ProductDraft, diff, validate_product},
    dto::{AddIdentifier, CreateProduct, IdentifierOut, ProductOut, UpdateProduct},
    repo::ProductRow,
};
use crate::{audit, shared::code::normalize_query};

const MAX_IDENTIFIER: usize = 100;

/// The 409 for an identifier someone else already holds.
async fn duplicate_error(
    db: &PgPool,
    brand_id: Uuid,
    identifier_type: &str,
    normalized: &str,
) -> AppError {
    let owner = match db.acquire().await {
        Ok(mut c) => repo::identifier_owner(&mut c, brand_id, identifier_type, normalized).await,
        Err(e) => Err(e.into()),
    };
    match owner {
        Ok(Some((product_id, name, archived))) => {
            let what = if identifier_type == "product_code" {
                "Code"
            } else {
                "That code"
            };
            let suffix = if archived { " (archived)" } else { "" };
            AppError::Duplicate {
                message: format!("{what} {normalized} is already used by {name}{suffix}."),
                details: json!({ "product_id": product_id, "name": name, "archived": archived }),
            }
        }
        Ok(None) => AppError::Conflict(format!("{normalized} is already in use.")),
        Err(e) => e,
    }
}

async fn check_category(conn: &mut PgConnection, id: Option<Uuid>) -> AppResult<()> {
    if let Some(id) = id
        && !repo::category_exists(conn, id).await?
    {
        return Err(AppError::FieldInvalid {
            field: "category_id".into(),
            message: "That category doesn't exist.".into(),
        });
    }
    Ok(())
}

fn to_out(row: ProductRow, identifiers: Vec<IdentifierOut>, can_see_cost: bool) -> ProductOut {
    ProductOut {
        id: row.id,
        name: row.name,
        description: row.description,
        brand: NamedRef {
            id: row.brand_id,
            name: row.brand_name,
        },
        category: row
            .category_id
            .zip(row.category_name)
            .map(|(id, name)| NamedRef { id, name }),
        code: row.code,
        base_code: row.base_code,
        variant: row.variant,
        unit: row.unit,
        pack_qty: row.pack_qty,
        attributes: row.attributes,
        selling_price: row.selling_price,
        mrp: row.mrp,
        default_cost: if can_see_cost { row.default_cost } else { None },
        hsn_code: row.hsn_code,
        archived: row.archived_at.is_some(),
        identifiers,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

pub async fn get_product(db: &PgPool, id: Uuid, can_see_cost: bool) -> AppResult<ProductOut> {
    let mut conn = db.acquire().await?;
    let row = repo::get_product(&mut conn, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let ids = repo::list_identifiers(&mut conn, id).await?;
    Ok(to_out(row, ids, can_see_cost))
}

pub async fn create_product(db: &PgPool, actor: Uuid, req: CreateProduct) -> AppResult<Uuid> {
    let draft = ProductDraft {
        name: req.name,
        code: req.code,
        category_id: req.category_id,
        description: req.description,
        unit: req.unit,
        pack_qty: req.pack_qty,
        attributes: req.attributes,
        selling_price: Some(req.selling_price),
        mrp: req.mrp,
        default_cost: req.default_cost,
        hsn_code: req.hsn_code,
    };
    let p = validate_product(&draft)?;
    let brand_id = req.brand_id.unwrap_or(EKTRA_BRAND_ID);

    let mut tx = db.begin().await?;
    if !repo::brand_exists(&mut tx, brand_id).await? {
        return Err(AppError::FieldInvalid {
            field: "brand_id".into(),
            message: "That brand doesn't exist.".into(),
        });
    }
    check_category(&mut tx, p.category_id).await?;
    let id = new_id();
    repo::insert_product(&mut tx, id, brand_id, &p, actor).await?;
    if let Err(e) = repo::upsert_code(&mut tx, id, brand_id, &p.raw_code, &p.code).await {
        drop(tx);
        return Err(if is_unique_violation(&e) {
            duplicate_error(db, brand_id, "product_code", &p.code.normalized).await
        } else {
            e.into()
        });
    }
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "product.create",
            entity_type: "product",
            entity_id: Some(id),
            before: None,
            after: Some(Value::Object(p.snapshot())),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn update_product(
    db: &PgPool,
    actor: Uuid,
    id: Uuid,
    patch: UpdateProduct,
) -> AppResult<()> {
    let mut tx = db.begin().await?;
    let row = repo::get_product(&mut tx, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let current = row.to_draft();
    let before = validate_product(&current)
        .map(|v| v.snapshot())
        .unwrap_or_default();

    let mut d = current;
    if let Some(v) = patch.name {
        d.name = v;
    }
    if let Some(v) = patch.code {
        d.code = v;
    }
    if let Some(v) = patch.category_id {
        d.category_id = v;
    }
    if let Some(v) = patch.description {
        d.description = v;
    }
    if let Some(v) = patch.unit {
        d.unit = Some(v);
    }
    if let Some(v) = patch.pack_qty {
        d.pack_qty = Some(v);
    }
    if let Some(v) = patch.attributes {
        d.attributes = Some(v);
    }
    if let Some(v) = patch.selling_price {
        d.selling_price = Some(v);
    }
    if let Some(v) = patch.mrp {
        d.mrp = v;
    }
    if let Some(v) = patch.default_cost {
        d.default_cost = v;
    }
    if let Some(v) = patch.hsn_code {
        d.hsn_code = v;
    }
    let p = validate_product(&d)?;
    check_category(&mut tx, p.category_id).await?;

    let code_changed = row.code.as_deref() != Some(p.code.normalized.as_str())
        || row.code_raw.as_deref() != Some(p.raw_code.as_str());
    if code_changed
        && let Err(e) = repo::upsert_code(&mut tx, id, row.brand_id, &p.raw_code, &p.code).await
    {
        drop(tx);
        return Err(if is_unique_violation(&e) {
            duplicate_error(db, row.brand_id, "product_code", &p.code.normalized).await
        } else {
            e.into()
        });
    }
    repo::update_product(&mut tx, id, &p).await?;
    let (b, a) = diff(&before, &p.snapshot());
    if !a.is_empty() {
        audit::record(
            &mut tx,
            audit::Entry {
                actor_id: Some(actor),
                action: "product.update",
                entity_type: "product",
                entity_id: Some(id),
                before: Some(Value::Object(b)),
                after: Some(Value::Object(a)),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Archive (`true`) or restore (`false`). Repeating it is a no-op.
pub async fn set_archived(db: &PgPool, actor: Uuid, id: Uuid, archived: bool) -> AppResult<()> {
    let mut tx = db.begin().await?;
    if repo::get_product(&mut tx, id, true).await?.is_none() {
        return Err(AppError::NotFound);
    }
    if repo::set_archived(&mut tx, id, archived).await? {
        audit::record(
            &mut tx,
            audit::Entry {
                actor_id: Some(actor),
                action: if archived {
                    "product.archive"
                } else {
                    "product.restore"
                },
                entity_type: "product",
                entity_id: Some(id),
                before: None,
                after: None,
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn add_identifier(
    db: &PgPool,
    actor: Uuid,
    product_id: Uuid,
    req: AddIdentifier,
) -> AppResult<IdentifierOut> {
    let t = req.identifier_type.as_str();
    if !matches!(t, "sku" | "alias") {
        return Err(AppError::FieldInvalid {
            field: "type".into(),
            message: "Add a shop code (sku) or another name (alias). Change the model code by editing the product.".into(),
        });
    }
    let raw = req.value.trim().to_string();
    let normalized = normalize_query(&raw);
    if normalized.is_empty() || raw.chars().count() > MAX_IDENTIFIER {
        return Err(AppError::FieldInvalid {
            field: "value".into(),
            message: format!("Enter 1 to {MAX_IDENTIFIER} characters."),
        });
    }
    let mut tx = db.begin().await?;
    let row = repo::get_product(&mut tx, product_id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let id = new_id();
    if let Err(e) =
        repo::insert_identifier(&mut tx, id, product_id, row.brand_id, t, &raw, &normalized).await
    {
        drop(tx);
        return Err(if is_unique_violation(&e) {
            duplicate_error(db, row.brand_id, t, &normalized).await
        } else {
            e.into()
        });
    }
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "product.identifier.add",
            entity_type: "product",
            entity_id: Some(product_id),
            before: None,
            after: Some(json!({ "type": t, "value": raw })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(IdentifierOut {
        id,
        identifier_type: t.to_string(),
        raw_value: raw,
        normalized_value: normalized,
    })
}

pub async fn remove_identifier(
    db: &PgPool,
    actor: Uuid,
    product_id: Uuid,
    id: Uuid,
) -> AppResult<()> {
    let mut tx = db.begin().await?;
    let (t, raw) = repo::identifier_type_of(&mut tx, product_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if t == "product_code" {
        return Err(AppError::FieldInvalid {
            field: "type".into(),
            message: "The model code can't be removed. Edit the product to change it.".into(),
        });
    }
    repo::delete_identifier(&mut tx, id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "product.identifier.remove",
            entity_type: "product",
            entity_id: Some(product_id),
            before: Some(json!({ "type": t, "value": raw })),
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
