//! Product search and the paged product list (Architecture.md §6.4).
//! The matching itself is the SQL function `catalog_search_hits` (see migrations).
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::dto::NamedRef;
use crate::{
    error::AppResult,
    shared::{
        code::normalize_query,
        paging::{Page, Paging},
    },
};

const SEARCH_DEFAULT_LIMIT: i64 = 20;
const SEARCH_MAX_LIMIT: i64 = 50;
/// Longest search text we look at; anything longer can't be a code or product name.
const MAX_QUERY: usize = 200;

#[derive(Debug, Serialize)]
pub struct ProductSummary {
    pub id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub base_code: Option<String>,
    pub variant: Option<String>,
    pub selling_price: Decimal,
    pub mrp: Option<Decimal>,
    pub unit: String,
    pub category: Option<NamedRef>,
    pub archived: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub product: ProductSummary,
    /// How it matched: `exact`, `base`, `prefix` or `name`.
    #[serde(rename = "match")]
    pub match_kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    Name,
    Code,
    Updated,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub q: Option<String>,
    #[serde(default)]
    pub status: Status,
    pub category_id: Option<Uuid>,
    pub brand_id: Option<Uuid>,
    pub sort: Option<Sort>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn stage_name(stage: Option<i32>) -> &'static str {
    match stage {
        Some(1) => "exact",
        Some(2) => "base",
        Some(3) => "prefix",
        _ => "name",
    }
}

/// `(raw, normalised)` search text, or `None` when there's nothing to search for.
fn query_text(q: Option<&str>) -> Option<(String, String)> {
    let raw: String = q?.trim().chars().take(MAX_QUERY).collect();
    if raw.is_empty() {
        return None;
    }
    let norm = normalize_query(&raw);
    Some((raw, norm))
}

struct Row {
    id: Uuid,
    name: String,
    code: Option<String>,
    base_code: Option<String>,
    variant: Option<String>,
    selling_price: Decimal,
    mrp: Option<Decimal>,
    unit: String,
    category_id: Option<Uuid>,
    category_name: Option<String>,
    archived: bool,
    stage: Option<i32>,
}

impl Row {
    fn summary(self) -> (ProductSummary, Option<i32>) {
        (
            ProductSummary {
                id: self.id,
                name: self.name,
                code: self.code,
                base_code: self.base_code,
                variant: self.variant,
                selling_price: self.selling_price,
                mrp: self.mrp,
                unit: self.unit,
                category: self
                    .category_id
                    .zip(self.category_name)
                    .map(|(id, name)| NamedRef { id, name }),
                archived: self.archived,
            },
            self.stage,
        )
    }
}

/// Ranked lookup over active products, for the POS and the Products search box.
pub async fn search(db: &PgPool, p: SearchParams) -> AppResult<SearchResponse> {
    let Some((raw, norm)) = query_text(p.q.as_deref()) else {
        return Ok(SearchResponse { results: vec![] });
    };
    let limit = p
        .limit
        .unwrap_or(SEARCH_DEFAULT_LIMIT)
        .clamp(1, SEARCH_MAX_LIMIT);
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT p.id, p.name, pi.normalized_value AS "code?", pi.base_code AS "base_code?",
                  pi.variant AS "variant?", p.selling_price, p.mrp, p.unit,
                  p.category_id, c.name AS "category_name?",
                  p.archived_at IS NOT NULL AS "archived!", h.stage AS "stage?"
           FROM catalog_search_hits($1, $2) h
           JOIN products p ON p.id = h.product_id
           LEFT JOIN categories c ON c.id = p.category_id
           LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
           WHERE p.archived_at IS NULL
           ORDER BY h.stage, pi.base_code, pi.variant_num NULLS FIRST, pi.variant, lower(p.name), p.id
           LIMIT $3"#,
        raw,
        norm,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(SearchResponse {
        results: rows
            .into_iter()
            .map(|r| {
                let (product, stage) = r.summary();
                SearchHit {
                    product,
                    match_kind: stage_name(stage),
                }
            })
            .collect(),
    })
}

/// Paged product list with optional search text and filters.
pub async fn list(db: &PgPool, p: ListParams) -> AppResult<Page<ProductSummary>> {
    let paging = Paging::from_query(p.page, p.per_page);
    let text = query_text(p.q.as_deref());
    let has_q = text.is_some();
    let (raw, norm) = text.unwrap_or_default();
    let status = match p.status {
        Status::Active => "active",
        Status::Archived => "archived",
        Status::All => "all",
    };
    // With search text and no explicit sort, best matches come first.
    let sort = match p.sort {
        Some(Sort::Name) => "name",
        Some(Sort::Code) => "code",
        Some(Sort::Updated) => "updated",
        None if has_q => "rank",
        None => "name",
    };

    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM products p
           LEFT JOIN catalog_search_hits($1, $2) h ON h.product_id = p.id
           WHERE (NOT $3 OR h.stage IS NOT NULL)
             AND ($4 = 'all' OR ($4 = 'active') = (p.archived_at IS NULL))
             AND ($5::uuid IS NULL OR p.category_id = $5)
             AND ($6::uuid IS NULL OR p.brand_id = $6)"#,
        raw,
        norm,
        has_q,
        status,
        p.category_id,
        p.brand_id
    )
    .fetch_one(db)
    .await?;

    let rows = sqlx::query_as!(
        Row,
        r#"SELECT p.id, p.name, pi.normalized_value AS "code?", pi.base_code AS "base_code?",
                  pi.variant AS "variant?", p.selling_price, p.mrp, p.unit,
                  p.category_id, c.name AS "category_name?",
                  p.archived_at IS NOT NULL AS "archived!", h.stage AS "stage?"
           FROM products p
           LEFT JOIN catalog_search_hits($1, $2) h ON h.product_id = p.id
           LEFT JOIN categories c ON c.id = p.category_id
           LEFT JOIN product_identifiers pi ON pi.product_id = p.id AND pi.identifier_type = 'product_code'
           WHERE (NOT $3 OR h.stage IS NOT NULL)
             AND ($4 = 'all' OR ($4 = 'active') = (p.archived_at IS NULL))
             AND ($5::uuid IS NULL OR p.category_id = $5)
             AND ($6::uuid IS NULL OR p.brand_id = $6)
           ORDER BY
             CASE WHEN $7 = 'rank' THEN h.stage END,
             CASE WHEN $7 IN ('rank', 'code') THEN pi.base_code END,
             CASE WHEN $7 IN ('rank', 'code') THEN pi.variant_num END NULLS FIRST,
             CASE WHEN $7 = 'updated' THEN p.updated_at END DESC,
             lower(p.name), pi.base_code, pi.variant_num NULLS FIRST, pi.variant, p.id
           LIMIT $8 OFFSET $9"#,
        raw,
        norm,
        has_q,
        status,
        p.category_id,
        p.brand_id,
        sort,
        paging.per_page,
        paging.offset()
    )
    .fetch_all(db)
    .await?;

    Ok(Page {
        items: rows.into_iter().map(|r| r.summary().0).collect(),
        page: paging.page,
        per_page: paging.per_page,
        total,
    })
}
