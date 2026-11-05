//! CSV product import with a dry run (plan Task 5). All or nothing: any row with a
//! problem stops the whole import, so a half-imported spreadsheet can't happen.
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    domain::{ProductDraft, ValidProduct, validate_product},
    is_unique_violation, repo,
};
use crate::{
    audit,
    error::{AppError, AppResult},
    shared::{code::normalize_query, ids::new_id},
};

pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_ROWS: usize = 2000;
const REQUIRED: &[&str] = &["code", "name", "selling_price"];
const OPTIONAL: &[&str] = &[
    "mrp",
    "cost",
    "unit",
    "category",
    "brand",
    "description",
    "aliases",
];
const MAX_ALIAS: usize = 100;

#[derive(Debug, Serialize)]
pub struct RowError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ImportRow {
    /// Spreadsheet row number: the header is row 1.
    pub row: usize,
    pub code: Option<String>,
    pub name: String,
    pub status: &'static str,
    /// Why a row is skipped.
    pub note: Option<String>,
    pub errors: Vec<RowError>,
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub rows: Vec<ImportRow>,
    pub created: usize,
    pub skipped: usize,
    pub new_categories: Vec<String>,
}

/// A row that will be created.
struct Planned {
    brand_id: Uuid,
    product: ValidProduct,
    category_key: Option<String>,
    aliases: Vec<(String, String)>,
}

/// The csv crate reports a record that follows blank lines at the first blank line;
/// step over the line breaks to the line the record really starts on.
fn record_line(text: &str, p: &csv::Position) -> usize {
    let mut line = p.line() as usize;
    for b in text.as_bytes().iter().skip(p.byte() as usize) {
        match b {
            b'\n' => line += 1,
            b'\r' => {}
            _ => break,
        }
    }
    line
}

fn invalid(msg: impl Into<String>) -> AppError {
    AppError::Validation(msg.into())
}

pub async fn import(
    db: &PgPool,
    actor: Uuid,
    body: &[u8],
    dry_run: bool,
) -> AppResult<ImportReport> {
    if body.len() > MAX_BYTES {
        return Err(invalid(
            "The file is larger than 1 MB. Split it into smaller files.",
        ));
    }
    let text = std::str::from_utf8(body).map_err(|_| {
        invalid("The file isn't UTF-8 text. Save it as \"CSV UTF-8\" and try again.")
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(false)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| invalid(format!("Couldn't read the header row: {e}")))?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let unknown: Vec<&str> = headers
        .iter()
        .map(String::as_str)
        .filter(|h| !REQUIRED.contains(h) && !OPTIONAL.contains(h))
        .collect();
    if !unknown.is_empty() {
        return Err(invalid(format!(
            "Unknown columns: {}. Allowed: {}.",
            unknown.join(", "),
            [REQUIRED, OPTIONAL].concat().join(", ")
        )));
    }
    let missing: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|r| !headers.iter().any(|h| h == r))
        .collect();
    if !missing.is_empty() {
        return Err(invalid(format!("Missing columns: {}.", missing.join(", "))));
    }
    let col = |name: &str| headers.iter().position(|h| h == name);

    let mut records = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| {
            let line = e.position().map(|p| p.line()).unwrap_or(0);
            invalid(format!("Row {line}: {e}"))
        })?;
        if rec.iter().all(str::is_empty) {
            continue;
        }
        // The spreadsheet line the record starts on (header = 1), so "Row N" matches
        // what people see even with blank lines or multi-line cells.
        let line = rec.position().map(|p| record_line(text, p)).unwrap_or(0);
        records.push((line, rec));
        if records.len() > MAX_ROWS {
            return Err(invalid(format!(
                "Import at most {MAX_ROWS} products at a time."
            )));
        }
    }

    // What already exists, keyed the way the database's unique indexes are.
    let mut conn = db.acquire().await?;
    let brands: HashMap<String, Uuid> = repo::list_brands(&mut conn)
        .await?
        .into_iter()
        .map(|b| (b.name.to_lowercase(), b.id))
        .collect();
    let categories: HashMap<String, Uuid> = repo::list_categories(&mut conn)
        .await?
        .into_iter()
        .map(|c| (c.name.to_lowercase(), c.id))
        .collect();
    let mut taken: HashSet<(Uuid, String, String)> = repo::all_identifier_keys(&mut conn)
        .await?
        .into_iter()
        .collect();
    drop(conn);

    let mut rows = Vec::new();
    let mut planned = Vec::new();
    // Codes planned earlier in this file → the row they're on.
    let mut in_file: HashMap<(Uuid, String), usize> = HashMap::new();
    let mut new_categories: BTreeMap<String, String> = BTreeMap::new();
    let (mut created, mut skipped) = (0, 0);

    for (row_no, rec) in records {
        let get = |name: &str| {
            col(name)
                .and_then(|i| rec.get(i))
                .map(str::to_string)
                .filter(|s| !s.is_empty())
        };
        let name = get("name").unwrap_or_default();
        let mut errors = Vec::new();
        let draft = ProductDraft {
            name: name.clone(),
            code: get("code").unwrap_or_default(),
            unit: get("unit"),
            selling_price: get("selling_price"),
            mrp: get("mrp"),
            default_cost: get("cost"),
            description: get("description"),
            ..Default::default()
        };
        let product = validate_product(&draft)
            .map_err(|e| {
                let field = if e.field == "default_cost" {
                    "cost"
                } else {
                    e.field
                };
                errors.push(RowError {
                    field: field.into(),
                    message: e.message,
                });
            })
            .ok();
        let brand_id = match get("brand") {
            None => Some(super::EKTRA_BRAND_ID),
            Some(b) => brands.get(&b.to_lowercase()).copied().or_else(|| {
                errors.push(RowError {
                    field: "brand".into(),
                    message: format!("No brand called {b}. Add it first."),
                });
                None
            }),
        };
        let code = product.as_ref().map(|p| p.code.normalized.clone());

        let (Some(product), Some(brand_id)) = (product, brand_id) else {
            rows.push(ImportRow {
                row: row_no,
                code,
                name,
                status: "error",
                note: None,
                errors,
            });
            continue;
        };
        let code_key = (
            brand_id,
            "product_code".to_string(),
            product.code.normalized.clone(),
        );
        if taken.contains(&code_key) {
            skipped += 1;
            let note = match in_file.get(&(brand_id, product.code.normalized.clone())) {
                Some(first) => format!("Same code as row {first}."),
                None => "Already in the catalogue.".to_string(),
            };
            rows.push(ImportRow {
                row: row_no,
                code,
                name,
                status: "skip",
                note: Some(note),
                errors,
            });
            continue;
        }

        let mut aliases = Vec::new();
        for raw in get("aliases")
            .unwrap_or_default()
            .split('|')
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            let norm = normalize_query(raw);
            let key = (brand_id, "alias".to_string(), norm.clone());
            if norm.is_empty() || raw.chars().count() > MAX_ALIAS {
                errors.push(RowError {
                    field: "aliases".into(),
                    message: format!("Alias {raw} is empty or too long."),
                });
            } else if taken.contains(&key) || aliases.iter().any(|(_, n)| n == &norm) {
                errors.push(RowError {
                    field: "aliases".into(),
                    message: format!("Alias {raw} is already used."),
                });
            } else {
                aliases.push((raw.to_string(), norm));
            }
        }
        if !errors.is_empty() {
            rows.push(ImportRow {
                row: row_no,
                code,
                name,
                status: "error",
                note: None,
                errors,
            });
            continue;
        }

        let category_key = get("category").map(|c| {
            let key = c.to_lowercase();
            if !categories.contains_key(&key) {
                new_categories.entry(key.clone()).or_insert(c);
            }
            key
        });
        taken.insert(code_key);
        in_file.insert((brand_id, product.code.normalized.clone()), row_no);
        for (_, norm) in &aliases {
            taken.insert((brand_id, "alias".to_string(), norm.clone()));
        }
        created += 1;
        rows.push(ImportRow {
            row: row_no,
            code,
            name,
            status: "create",
            note: None,
            errors,
        });
        planned.push(Planned {
            brand_id,
            product,
            category_key,
            aliases,
        });
    }

    let report = ImportReport {
        created,
        skipped,
        new_categories: new_categories.values().cloned().collect(),
        rows,
    };
    if dry_run {
        return Ok(report);
    }
    if report.rows.iter().any(|r| r.status == "error") {
        return Err(AppError::Unprocessable {
            message: "Some rows have problems. Fix them and try again; nothing was imported."
                .into(),
            details: serde_json::to_value(&report).unwrap_or(Value::Null),
        });
    }

    let mut tx = db.begin().await?;
    let mut category_ids = categories;
    for (key, name) in &new_categories {
        let id = new_id();
        repo::insert_category(&mut tx, id, name)
            .await
            .map_err(conflict)?;
        category_ids.insert(key.clone(), id);
    }
    for mut p in planned {
        p.product.category_id = p
            .category_key
            .as_ref()
            .and_then(|k| category_ids.get(k).copied());
        let id = new_id();
        repo::insert_product(&mut tx, id, p.brand_id, &p.product, actor)
            .await
            .map_err(conflict)?;
        repo::upsert_code(
            &mut tx,
            id,
            p.brand_id,
            &p.product.raw_code,
            &p.product.code,
        )
        .await
        .map_err(conflict)?;
        for (raw, norm) in &p.aliases {
            repo::insert_identifier(&mut tx, new_id(), id, p.brand_id, "alias", raw, norm)
                .await
                .map_err(conflict)?;
        }
    }
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(actor),
            action: "product.import",
            entity_type: "product",
            entity_id: None,
            before: None,
            after: Some(json!({
                "created": report.created,
                "skipped": report.skipped,
                "new_categories": report.new_categories,
            })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(report)
}

/// Someone added a clashing code or category between the check and the write.
fn conflict(e: sqlx::Error) -> AppError {
    if is_unique_violation(&e) {
        AppError::Conflict(
            "Products changed while importing. Nothing was imported; run the import again.".into(),
        )
    } else {
        e.into()
    }
}
