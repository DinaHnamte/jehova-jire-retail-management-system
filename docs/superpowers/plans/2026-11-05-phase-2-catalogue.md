# Phase 2 — Catalogue and Code Lookup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Staff can add, edit, archive and import products, and find any product by typing part of its printed model code or name, with results grouped by model and paged lists sized for ~500 products.

**Architecture:** A new `catalog` backend module (routes → service → domain → repo, Architecture.md §5.1) owning `brands`, `categories`, `products`, `product_identifiers`. One code normaliser, written in Rust and ported to TypeScript, both run against one shared fixtures file. Search is SQL in four ranked stages (§6.4). Frontend gets a `products` module: list with paging, detail, form, CSV import.

**Tech Stack:** Existing Axum/SQLx/Postgres + Vue 3/Pinia/Tailwind. New crates: `rust_decimal` (sqlx feature `rust_decimal`, serde as string), `unicode-normalization`, `csv`. No new frontend dependencies.

**Spec:** [Architecture.md](../../../Architecture.md) §6 (catalogue), §13, §14, §15, §18 rule 2, §20 phase 2; [ADR-0003](../../adr/0003-product-identification-without-barcodes.md); [fixtures/labels/README.md](../../../fixtures/labels/README.md).

## Global Constraints

- Everything from the Phase 1 plan still holds: UUIDv7 keys, `timestamptz`, SQLx macros only, standard error body, permission codes never role names, `ON DELETE RESTRICT`.
- Money columns `NUMERIC(12,2)`; in JSON, money is a **string** with two decimals (`"750.00"`), never a float. Valid range `0.00`–`9999999.99`, at most two decimal places.
- Permissions (already seeded): `products.read` view; `products.create` create products, brands, categories, import; `products.update` edit, identifiers; `products.archive` archive/restore.
- Products are never deleted; archived products keep their codes reserved.
- Creating or importing a product never creates stock (§18 rule 2). This phase has no stock tables or columns; the rule's test is written in Phase 4, when stock exists.
- The model code identifies a product; design/colour codes (`MP4952`, `MG4705`) are not stored as identifiers (ADR-0003).
- Audit (`audit::record`) on product create, update (with before/after of changed fields), archive, restore, identifier add/remove, and import (one row per import with counts).
- Paging: `page` ≥ 1 (default 1), `per_page` 1–100 (default 25); out-of-range values are clamped, not rejected.
- Search target: under 100 ms server time at 10,000 products (§6.4, phase 2 exit).
- UI copy: plain sentence case, actions named for what they do ("Add product", "Archive product"); prices shown as `₹1,090.00` (en-IN grouping).
- After any `sqlx::query!` change: `cargo sqlx prepare` and commit `.sqlx/` (sqlx-cli 0.9 with postgres, see Phase 1 ledger ruling for the local tool path).

## Scope rulings (made while planning)

- **No `product_images` or `attribute_vocabulary` tables yet.** Images are optional and vocabulary feeds the Phase 3 parser; both land with the phase that uses them.
- **No `mode=ocr` search yet.** OCR-tolerant expansion only has a caller in Phase 3; it is added there with the scanner.
- **`mrp` is a product column.** §6.1 doesn't list it, but ADR-0003 compares scanned MRP with "the saved MRP", and Indian law forbids selling above MRP, so the API enforces `selling_price ≤ mrp` when MRP is set.
- **The product's code lives only in `product_identifiers`** (type `product_code`, at most one per product). Responses expose it as `code`/`base_code`/`variant`. One source of truth instead of §6.1's duplicated columns.
- **Import creates only.** Rows whose code already exists are reported as `skip`, not updated. Bulk price updates are a later feature.
- **Categories are flat** (no parent) until someone needs a tree.

## Review Focus

1. **Same code typed differently** (`ip 1060 – 11″` vs `IP 1060-11"`) → treated as the same code: 409 `DUPLICATE_CODE` naming the existing product. (Task 3 `duplicate_code_in_other_spelling_conflicts`.)
2. **Two people create the same code at once** → exactly one 201 and one 409, never a 500. (Task 3 `concurrent_create_same_code_one_wins`.)
3. **Selling price above MRP** → 422 with a message naming the MRP; price exactly at MRP is fine. (Task 3 `selling_price_above_mrp_is_rejected`.)
4. **Search text with `%`, `_`, `\`, quotes or only spaces** → treated literally, 200 with sensible results, never a 500. (Task 4 `search_treats_wildcards_literally`.)
5. **Archived products** → hidden from search and the default list, shown with `status=archived`, and their code still blocks a new product with the same code. (Task 4 `archived_products_hidden_but_code_reserved`.)

---

## File structure

```text
fixtures/codes.json                         shared normaliser cases (Rust + TS)
backend/
  Cargo.toml                                + rust_decimal, unicode-normalization, csv
  migrations/20261106000000_catalog.sql     brands, categories, products, product_identifiers, seed brand
  src/error.rs                              + AppError::Duplicate
  src/shared/code.rs                        normalize_code, normalize_query
  src/shared/money.rs                       Money parsing/validation helpers
  src/shared/paging.rs                      Paging { page, per_page } + Page<T>
  src/catalog/{mod,routes,service,domain,repo,dto,import,search}.rs
  tests/common/mod.rs                       + patch_json, delete_with, post_raw
  tests/{codes,catalog_products,catalog_search,catalog_import}.rs
frontend/src/
  shared/api/client.ts                      + optional AbortSignal on get
  shared/code.ts (+ .test.ts)               TS port of the normaliser
  shared/money.ts (+ .test.ts)              formatRupees, parseMoneyInput
  modules/products/api.ts                   types + calls
  modules/products/list-query.ts (+ .test.ts)  route query <-> list params
  modules/products/{ProductsPage,ProductDetailPage,ProductFormPage,ImportPage}.vue
  app/nav.ts, app/layouts/NavIcon.vue, app/router.ts
```

---

### Task 1: Code normaliser (Rust + TypeScript, shared fixtures)

**Files:**
- Create: `fixtures/codes.json`, `backend/src/shared/code.rs`, `backend/tests/codes.rs`, `frontend/src/shared/code.ts`, `frontend/src/shared/code.test.ts`
- Modify: `backend/Cargo.toml` (+ `unicode-normalization = "0.1"`), `backend/src/shared/mod.rs`

**Interfaces:**
- Produces (Rust, `jj_retail_api::shared::code`):
  - `pub struct ProductCode { pub normalized: String, pub base: String, pub variant: Option<String>, pub digits: String }`
  - `pub fn normalize_code(raw: &str) -> Option<ProductCode>`
  - `pub fn normalize_query(q: &str) -> String`
- Produces (TS, `@/shared/code`): `normalizeCode(raw: string): ProductCode | null` with the same fields (`variant: string | null`), and `normalizeQuery(q: string): string`.

Rules (both languages): NFKC; map every dash (`‐ ‑ ‒ – — ― − ﹣ －`) to `-`; uppercase; remove all whitespace; strip trailing inch marks (`"`, `″`, `”`, `''`); trim `-` at both ends. `normalize_code` returns `None` if the result is empty or has no ASCII digit. `base` = text before the first `-`; `variant` = text after it, `None` if absent or empty. `digits` = `normalized` with its leading letters removed (`B6275-4.75` → `6275-4.75`), used for digit-prefix search. `normalize_query` applies the same steps but never fails (may return `""`).

- [ ] **Step 1: Write `fixtures/codes.json`** — an array of `{ "raw", "normalized", "base", "variant", "digits" }`, `null` fields for rejected input. Cases:

| raw | normalized | base | variant | digits |
|---|---|---|---|---|
| `B 6275-4.75` | `B6275-4.75` | `B6275` | `4.75` | `6275-4.75` |
| `IP 1060-11"` | `IP1060-11` | `IP1060` | `11` | `1060-11` |
| `IB 1290 -7.8"` | `IB1290-7.8` | `IB1290` | `7.8` | `1290-7.8` |
| `B 6143-8` | `B6143-8` | `B6143` | `8` | `6143-8` |
| `b6143–8` (en dash) | `B6143-8` | `B6143` | `8` | `6143-8` |
| `Ｂ６１４３－８` (full-width) | `B6143-8` | `B6143` | `8` | `6143-8` |
| `IP 1060-11″` (double prime) | `IP1060-11` | `IP1060` | `11` | `1060-11` |
| `B6275` | `B6275` | `B6275` | null | `6275` |
| `B6275-` | `B6275` | `B6275` | null | `6275` |
| `"   "` | null | | | |
| `Plate` | null | | | |

- [ ] **Step 2: Write failing tests.** `backend/tests/codes.rs::matches_shared_fixtures` loads `../fixtures/codes.json` (path from `CARGO_MANIFEST_DIR`) and asserts every case; `normalize_query_never_fails` asserts `normalize_query(" ip 1060 – 11″ ") == "IP1060-11"` and `normalize_query("   ") == ""`. `frontend/src/shared/code.test.ts` imports the same JSON (`../../../fixtures/codes.json`) and asserts the same cases plus the two query cases.
- [ ] **Step 3: Run both** — `cd backend && cargo test --test codes`; `cd frontend && pnpm vitest run src/shared/code.test.ts`. Expected: FAIL (module missing).
- [ ] **Step 4: Implement** `shared/code.rs` and `shared/code.ts` per the rules above. If Vite refuses the import outside `frontend/`, add `server.fs.allow: ['..']` to `vite.config.ts` (record a ruling).
- [ ] **Step 5: Run both** — Expected: PASS. Run `cargo clippy --all-targets -- -D warnings` and `pnpm typecheck`.
- [ ] **Step 6: Commit** — `git commit -m "catalog: product code normaliser (Rust + TS, shared fixtures)"`

### Task 2: Catalogue schema, brands and categories

**Files:**
- Create: `backend/migrations/20261106000000_catalog.sql`, `backend/src/catalog/{mod,routes,service,repo,dto}.rs`, `backend/src/shared/{money,paging}.rs`, `backend/tests/catalog_products.rs` (brands/categories tests go here)
- Modify: `backend/Cargo.toml` (+ `rust_decimal = { version = "1", features = ["serde-str"] }`, sqlx feature `rust_decimal`), `backend/src/lib.rs`, `backend/src/http/mod.rs` (nest `catalog::routes::router()` under `/api/v1`), `backend/src/error.rs`, `backend/tests/common/mod.rs`, `backend/tests/schema.rs`

**Interfaces:**
- Produces:
  - Tables: `brands(id, name UNIQUE on lower(name), created_at)`; `categories(id, name UNIQUE on lower(name), created_at)`; `products(id, brand_id → brands, category_id → categories NULL, name TEXT NOT NULL, description TEXT, unit TEXT NOT NULL DEFAULT 'piece' CHECK (unit IN ('piece','set','pack','box','pair','dozen')), pack_qty INTEGER NOT NULL DEFAULT 1 CHECK (pack_qty > 0), attributes JSONB NOT NULL DEFAULT '{}', selling_price NUMERIC(12,2) NOT NULL CHECK (selling_price >= 0), mrp NUMERIC(12,2) CHECK (mrp >= 0), default_cost NUMERIC(12,2) CHECK (default_cost >= 0), hsn_code TEXT, tax_category TEXT, archived_at TIMESTAMPTZ, created_by → users, created_at, updated_at)`; `product_identifiers` as in §14 plus `digits TEXT`, `variant TEXT`, `variant_num NUMERIC` (numeric value of the variant for sorting, NULL if not a number), `created_at`. Indexes: `UNIQUE (brand_id, identifier_type, normalized_value)`; partial `UNIQUE (product_id) WHERE identifier_type = 'product_code'`; `text_pattern_ops` on `normalized_value` and on `digits`; GIN `gin_trgm_ops` on `products.name` and on `product_identifiers.normalized_value`; btree on `products(archived_at)`. All FKs `ON DELETE RESTRICT`.
  - Seed brand `Ektra by Superware` with fixed id `00000000-0000-7000-8001-000000000001`.
  - `AppError::Duplicate { message: String, details: serde_json::Value }` → 409, code `DUPLICATE_CODE`, `details` rendered in the error body.
  - `shared::money::parse_money(s: &str) -> Result<Decimal, String>` (accepts `"750"`, `"750.5"`, `"1,090.00"`; rejects negatives, >2 dp, > 9999999.99, non-numbers; the `Err` is a user-facing message).
  - `shared::paging::{Paging::from_query(page: Option<i64>, per_page: Option<i64>) -> Paging, Paging { page: i64, per_page: i64 }, Paging::offset(&self) -> i64, Page<T> { items: Vec<T>, page, per_page, total: i64 }}` (Serialize).
  - Endpoints: `GET /brands`, `POST /brands {name}`, `GET /categories`, `POST /categories {name}` → `{id, name}`; lists sorted by name; read needs `products.read`, create needs `products.create`; duplicate name (any case) → 409 `CONFLICT`; empty/over-100-char name → 422.
  - Test helpers: `TestApp::patch_json(path, body, cookie, csrf)`, `TestApp::delete_with(path, cookie, csrf)`, `TestApp::post_raw(path, content_type, body: String, cookie, csrf)`.

- [ ] **Step 1: Write failing tests** in `tests/catalog_products.rs`: `brands_list_includes_seeded_ektra` (GET `/api/v1/brands` as a cashier → contains `"Ektra by Superware"`); `cashier_cannot_create_category` (403 `FORBIDDEN`); `stock_operator_creates_category_and_duplicate_conflicts` (201 then `"plates"` → 409); `category_name_must_not_be_blank` (`"  "` → 422). Unit tests in `shared/money.rs`: `parse_money_accepts_common_forms` (`"750"`→750.00, `"1,090.00"`→1090.00, `"0"`→0.00) and `parse_money_rejects_bad_values` (`"-1"`, `"1.234"`, `"10000000"`, `"abc"`, `""`). Unit test in `shared/paging.rs`: `paging_clamps` (`(None,None)`→1/25, `(Some(0),Some(0))`→1/1, `(Some(-5),Some(1000))`→1/100). Extend `tests/schema.rs` with `catalog_tables_exist`.
- [ ] **Step 2: Run** `cargo test --test catalog_products --test schema && cargo test --lib` — Expected: FAIL.
- [ ] **Step 3: Implement** the migration, error variant, money, paging, and brand/category routes (thin service/repo per §5.1).
- [ ] **Step 4: Run the full backend suite** — `cargo test` — Expected: PASS (59 earlier tests + new). Then `cargo sqlx prepare`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** — `git commit -m "catalog: schema, brands and categories"`

### Task 3: Products and identifiers (create, read, update, archive)

**Files:**
- Create: `backend/src/catalog/domain.rs`
- Modify: `backend/src/catalog/{routes,service,repo,dto}.rs`, `backend/tests/catalog_products.rs`

**Interfaces:**
- Consumes: Task 1 `normalize_code`; Task 2 tables, `parse_money`, `AppError::Duplicate`, test helpers.
- Produces:
  - `ProductOut { id, name, description, brand: {id,name}, category: {id,name} | null, code: String | null, base_code, variant, unit, pack_qty, attributes, selling_price: String, mrp: String | null, default_cost: String | null, hsn_code, archived: bool, identifiers: [{id, type, raw_value, normalized_value}], created_at, updated_at }` — `default_cost` is `null` unless the caller has `products.update`.
  - `POST /products` (`products.create`) body `{ name, code, brand_id?, category_id?, description?, unit?, pack_qty?, attributes?, selling_price, mrp?, default_cost?, hsn_code? }` (money fields are strings) → 201 `ProductOut`. `brand_id` defaults to the Ektra id.
  - `GET /products/{id}` (`products.read`) → `ProductOut`, 404 if unknown.
  - `PATCH /products/{id}` (`products.update`) — any subset of the create fields; changing `code` re-normalises and re-checks uniqueness → `ProductOut`.
  - `POST /products/{id}/archive`, `POST /products/{id}/restore` (`products.archive`) → `ProductOut`; archiving twice is a no-op 200.
  - `POST /products/{id}/identifiers` (`products.update`) `{ type: "sku" | "alias", value }` → 201 identifier; `DELETE /products/{id}/identifiers/{iid}` → 204; the `product_code` identifier can't be added or removed this way (422).
  - `domain::validate_product(input) -> Result<ValidProduct, AppError>`: name trimmed 1–200 chars; code must normalise; `selling_price` required; `selling_price ≤ mrp` when `mrp` is set (message: `"Selling price can't be more than the MRP (₹750.00)."`); unit in the allowed list.
  - Duplicate code → `AppError::Duplicate { message: "Code IP1060-11 is already used by 4 Section Plate.", details: { product_id, name, archived } }`.

- [ ] **Step 1: Write failing tests** in `tests/catalog_products.rs` (use a `stock_operator` user for writes, a `cashier` for reads):
  - `create_product_returns_normalised_code` — POST `{name:"4 Section Plate", code:"IP 1060-11\"", selling_price:"700", mrp:"750"}` → 201; `code == "IP1060-11"`, `base_code == "IP1060"`, `variant == "11"`, `selling_price == "700.00"`, `brand.name == "Ektra by Superware"`, `archived == false`.
  - `duplicate_code_in_other_spelling_conflicts` — create `IP 1060-11"`, then `ip 1060 – 11″` → 409 `DUPLICATE_CODE`, `details.name == "4 Section Plate"`.
  - `concurrent_create_same_code_one_wins` — two creates of the same code joined with `tokio::join!` → statuses sorted `[201, 409]`.
  - `selling_price_above_mrp_is_rejected` — `selling_price:"751", mrp:"750"` → 422 and message contains `"₹750.00"`; `"750"`/`"750"` → 201.
  - `money_must_be_a_string_amount` — `selling_price: "1.234"` → 422; `selling_price: -5` (number) → 422 or 400 (not 500).
  - `cashier_cannot_create_or_see_cost` — cashier POST → 403; cashier GET of a product with `default_cost:"400"` → `default_cost == null`; stock operator GET → `"400.00"`.
  - `patch_updates_price_and_audits` — PATCH `{selling_price:"720"}` → 200 `"720.00"`; an `audit_logs` row with `action = 'product.update'` has `before.selling_price == "700.00"`, `after.selling_price == "720.00"`.
  - `archive_and_restore` — archive → `archived == true`; again → 200; restore → `false`; cashier archive → 403.
  - `aliases_can_be_added_and_removed` — add alias `"4 sec plate"` → 201; GET lists it; DELETE → 204; adding type `product_code` → 422.
  - `unknown_product_is_404` — GET with a random UUID → 404 `NOT_FOUND`.
- [ ] **Step 2: Run** `cargo test --test catalog_products` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement** domain validation, repo, service (one transaction per write, audit inside it), routes. Map Postgres unique violation `23505` on `product_identifiers` to `AppError::Duplicate` by looking up the existing product in a fresh query after rollback.
- [ ] **Step 4: Run** `cargo test` — Expected: PASS. `cargo sqlx prepare`, fmt, clippy.
- [ ] **Step 5: Commit** — `git commit -m "catalog: products and identifiers API"`

### Task 4: Paged list and ranked search

**Files:**
- Create: `backend/src/catalog/search.rs`, `backend/tests/catalog_search.rs`
- Modify: `backend/src/catalog/{routes,repo,dto}.rs`

**Interfaces:**
- Consumes: Task 1 `normalize_query`; Task 2 `Paging`, `Page<T>`; Task 3 products.
- Produces:
  - `ProductSummary { id, name, code, base_code, variant, selling_price, mrp, unit, category: {id,name} | null, archived }`.
  - `GET /products/search?q=&limit=` (`products.read`) → `{ results: [ProductSummary & { match: "exact" | "base" | "prefix" | "name" }] }`. `limit` 1–50, default 20. Active products only. Empty/whitespace `q` → `{results: []}`. Ranking: stage (exact < base < prefix < name), then `base_code`, then `variant_num` (nulls first), then `variant`, then name. Stages per §6.4 using `Q = normalize_query(q)`: (1) `normalized_value = Q`; (2) `base_code = Q`; (3) `normalized_value LIKE Q || '%'` or `digits LIKE Q || '%'` (escape `%`, `_`, `\` in `Q`); (4) `word_similarity(q, products.name) >= 0.35` or `products.name ILIKE '%' || q || '%'` (escaped), also matching alias identifiers. A product appears once, at its best stage.
  - `GET /products?q=&status=active|archived|all&category_id=&brand_id=&sort=name|code|updated&page=&per_page=` (`products.read`) → `Page<ProductSummary>`. Default `status=active`, `sort=name` (or search rank when `q` is set and `sort` isn't). `total` counts all matches before paging.

- [ ] **Step 1: Write failing tests** in `tests/catalog_search.rs` (seed via the API or repo: `IP1060-11` "4 Section Plate", `IP1060-9` "4 Section Plate 9in", `B6143-8` "Storage Box", `B6143-10` "Storage Box", `B6275-4.75` "Round Bowl", `IB1290-7.8` "Easy Lunch"):
  - `exact_code_comes_first` — `q="ip 1060-11"` → `results[0].code == "IP1060-11"` with `match == "exact"`; no `B6143-*` or `IB1290-*` in the results.
  - `base_code_returns_all_sizes_in_size_order` — `q="B6143"` → `[B6143-8, B6143-10]` (numeric order, not text order), both `match == "base"`.
  - `digit_prefix_finds_model` — `q="614"` → both `B6143-*`, `match == "prefix"`; `q="1290"` → `IB1290-7.8`.
  - `name_search_finds_by_words` — `q="lunch"` → `IB1290-7.8` with `match == "name"`; `q="storage"` → both boxes.
  - `search_treats_wildcards_literally` — `q` in `["%", "_", "\\", "'", "100%", "   "]` → each 200; `"%"` and `"_"` return no results (no product name contains them); `"   "` → `results == []`.
  - `archived_products_hidden_but_code_reserved` — archive `B6275-4.75`; search `"6275"` → empty; `GET /products?status=archived` → contains it; default list doesn't; POST a new product with code `B 6275-4.75` → 409 with `details.archived == true`.
  - `list_pages_and_counts` — seed 30 products; `per_page=25&page=2` → 5 items, `total == 30`; `page=9` → `items == []`, `total == 30`; `per_page=1000` → `per_page == 100`.
  - `list_filters_by_category_and_q` — category filter returns only that category; `q="storage"` → `total == 2`.
  - `search_is_fast_at_10k_products` — insert 10,000 products with identifiers in one `INSERT … SELECT generate_series` (codes `T{n}-{n%7}`), `ANALYZE`, then time 20 searches over the HTTP app (`q` cycling through an exact code, a base, a 3-digit prefix, a word) and assert the median is under 100 ms. Mark it `#[ignore]` and run it in CI as a separate step (`cargo test --test catalog_search -- --ignored`) so local runs stay quick.
- [ ] **Step 2: Run** `cargo test --test catalog_search` — Expected: FAIL.
- [ ] **Step 3: Implement** search SQL as one query with a `CASE` stage column and `DISTINCT ON (product_id)` picking the best stage, wrapped for ordering and limit; reuse it as the filter for the list when `q` is set.
- [ ] **Step 4: Run** `cargo test` and `cargo test --test catalog_search -- --ignored` — Expected: PASS, perf median printed and < 100 ms. Add the ignored-test step to `.github/workflows/ci.yml` backend job. `cargo sqlx prepare`, fmt, clippy.
- [ ] **Step 5: Commit** — `git commit -m "catalog: paged product list and ranked code search"`

### Task 5: CSV import

**Files:**
- Create: `backend/src/catalog/import.rs`, `backend/tests/catalog_import.rs`
- Modify: `backend/Cargo.toml` (+ `csv = "1"`), `backend/src/catalog/routes.rs`

**Interfaces:**
- Consumes: Task 3 `validate_product`, repo insert, duplicate detection; Task 2 category creation.
- Produces: `POST /products/import?dry_run=true|false` (`products.create`), body `text/csv` (UTF-8, optional BOM), max 1 MB / 2,000 data rows (else 422). Columns by header name, case-insensitive, any order: required `code`, `name`, `selling_price`; optional `mrp`, `cost`, `unit`, `category`, `brand`, `description`, `aliases` (`|`-separated). Unknown headers → 422 listing them. Response `ImportReport { rows: [{ row: usize (1-based, header = row 1), code, name, status: "create" | "skip" | "error", errors: [{field, message}] }], created: usize, skipped: usize, new_categories: [String] }`. `skip` = code already exists (active or archived) or repeats an earlier row in the same file. Unknown `brand` → row error; unknown `category` → created. With `dry_run=false`: if any row has `status == "error"`, return 422 with the report and write nothing; otherwise insert everything in **one transaction** and write one audit row `product.import` with counts.

- [ ] **Step 1: Write failing tests**:
  - `dry_run_reports_without_writing` — 3 valid rows → 200, `created == 3`, then `GET /products` total is 0.
  - `import_creates_products_and_categories` — rows with categories `Plates`, `Boxes` → `created == 2`, `new_categories == ["Boxes","Plates"]`, products searchable by code.
  - `existing_and_repeated_codes_are_skipped` — one existing code, one code repeated twice in the file → statuses `skip`, `skipped == 2`.
  - `any_error_blocks_the_whole_import` — one row with `selling_price` above `mrp` and one valid row, `dry_run=false` → 422, report row 3 has `errors[0].field == "selling_price"`, and nothing was created.
  - `header_is_flexible_and_bom_is_ignored` — BOM + headers `Name,CODE,Selling_Price` in that order → works.
  - `missing_required_column_is_422` — no `code` header → 422 naming `code`.
  - `cashier_cannot_import` — 403.
- [ ] **Step 2: Run** `cargo test --test catalog_import` — Expected: FAIL.
- [ ] **Step 3: Implement** parsing (csv crate, `flexible(false)`, trim fields), per-row validation reusing `validate_product`, and the all-or-nothing write.
- [ ] **Step 4: Run** `cargo test` — Expected: PASS. Prepare, fmt, clippy.
- [ ] **Step 5: Commit** — `git commit -m "catalog: CSV product import with dry run"`

### Task 6: Frontend — money/format helpers, products API, paged list page

**Files:**
- Create: `frontend/src/shared/money.ts` (+ `.test.ts`), `frontend/src/modules/products/api.ts`, `frontend/src/modules/products/list-query.ts` (+ `.test.ts`), `frontend/src/modules/products/ProductsPage.vue`
- Modify: `frontend/src/shared/api/client.ts` (+ `.test.ts`), `frontend/src/app/nav.ts` (+ test), `frontend/src/app/layouts/NavIcon.vue`, `frontend/src/app/router.ts`

**Interfaces:**
- Consumes: Task 4 endpoints and shapes; Task 2 `/categories`.
- Produces:
  - `api.get<T>(path, opts?: { signal?: AbortSignal })`; an aborted request rejects with a `DOMException` named `AbortError` (not converted to `ApiError`).
  - `formatRupees(amount: string | null): string` — `"1090.00"` → `"₹1,090.00"`, `"0.00"` → `"₹0.00"`, `null` → `"—"`; uses `Intl.NumberFormat('en-IN', { style: 'currency', currency: 'INR' })` on the string without float rounding errors for values up to 9,999,999.99.
  - `parseMoneyInput(s: string): string | null` — `"1,090"` → `"1090.00"`, `"750.5"` → `"750.50"`, `" "` → `null`, `"abc"` / `"-1"` / `"1.234"` → `null`.
  - `products/api.ts`: types `ProductSummary`, `Product`, `Page<T>`, `Category`, `ImportReport`; functions `listProducts(params, signal?)`, `searchProducts(q, signal?)`, `getProduct(id)`, `createProduct(body)`, `updateProduct(id, body)`, `archiveProduct(id)`, `restoreProduct(id)`, `addIdentifier(id, type, value)`, `removeIdentifier(id, iid)`, `listCategories()`, `createCategory(name)`, `importProducts(csv: string, dryRun: boolean)`.
  - `list-query.ts`: `interface ListParams { q: string; status: 'active' | 'archived' | 'all'; categoryId: string | null; page: number; perPage: number }`, `fromRouteQuery(query: LocationQuery): ListParams` (defaults `''`, `'active'`, `null`, `1`, `25`; invalid values fall back to defaults), `toRouteQuery(p: ListParams): Record<string, string>` (omits defaults), `toApiQuery(p: ListParams): string`.
  - Route `/products` → `ProductsPage.vue`, meta `{ perm: 'products.read', title: 'Products' }`; nav item `{ label: 'Products', to: '/products', perm: 'products.read', icon: 'products' }` placed after Overview.
  - `ProductsPage.vue`: search box (debounced 120 ms, aborts the previous request, `/` focuses it), status tabs (Active / Archived / All), category select, results as a minimal table on `sm+` and stacked rows on phones (name, code, selling price, MRP, category), pager showing `"1–25 of 512"` with Previous/Next, state kept in the URL query so Back restores it, rows link to `/products/:id`. Empty states: no products yet → "Add product" and "Import from a spreadsheet" buttons (only if `products.create`); no matches → "No products match "<q>"." Uses existing tokens/components (`card`, `field`, `btn-primary`, `btn-secondary`, `Badge`).
- [ ] **Step 1: Write failing tests**: `money.test.ts` (cases above, including `formatRupees("9999999.99") === "₹99,99,999.99"`); `list-query.test.ts` (`fromRouteQuery({page:'3', status:'archived'})` → page 3/archived; `{page:'-2', perPage:'x', status:'bogus'}` → defaults; `toRouteQuery(defaults)` → `{}`; round trip preserves values); `client.test.ts` `passes the abort signal and rejects with AbortError`; `nav.test.ts` `shows Products to anyone with products.read`.
- [ ] **Step 2: Run** `pnpm test` — Expected: new tests FAIL.
- [ ] **Step 3: Implement** helpers, API module, route, nav icon (`products`: a box outline), and the page.
- [ ] **Step 4: Run** `pnpm typecheck && pnpm test && pnpm build` — Expected: all PASS. Manually check the page against the local API at 390 px and 1280 px, light and dark (screenshots like Phase 1's design pass).
- [ ] **Step 5: Commit** — `git commit -m "web: products list with search and paging"`

### Task 7: Frontend — product detail and form

**Files:**
- Create: `frontend/src/modules/products/ProductDetailPage.vue`, `frontend/src/modules/products/ProductFormPage.vue`, `frontend/src/modules/products/form.ts` (+ `.test.ts`)
- Modify: `frontend/src/app/router.ts`

**Interfaces:**
- Consumes: Task 6 API module and money helpers; Task 1 `normalizeCode`.
- Produces:
  - Routes: `/products/new` (`products.create`), `/products/:id` (`products.read`), `/products/:id/edit` (`products.update`).
  - `form.ts`: `interface ProductForm { name; code; categoryId; unit; packQty; sellingPrice; mrp; defaultCost; description; hsnCode }` (strings), `validateForm(f: ProductForm): Record<string, string>` (field → message; mirrors the server rules: name required ≤200, code must normalise, selling price required and a valid amount, selling price ≤ MRP with the message `"Selling price can't be more than the MRP (₹750.00)."`), `toRequestBody(f): object` (money via `parseMoneyInput`, empty optionals omitted).
  - Form page: live preview under the code field showing the normalised code (`Saved as IP1060-11`) using `normalizeCode`; server 409 `DUPLICATE_CODE` shows "Already used by <name>" with a link to that product; 422 messages shown on the matching field when `details.field` is present, otherwise above the form. Category select with an inline "New category" option.
  - Detail page: name, code, prices (cost only if returned), category, unit, identifiers list with "Add alias"/remove (if `products.update`), Edit button (`products.update`), Archive/Restore (`products.archive`) with a confirm step. No stock figures.
- [ ] **Step 1: Write failing tests** in `form.test.ts`: `requires name, code and selling price`; `rejects price above MRP with the MRP in the message`; `accepts price equal to MRP`; `toRequestBody normalises money and drops empty optionals` (`sellingPrice:"1,090"` → `selling_price:"1090.00"`, empty `mrp` absent).
- [ ] **Step 2: Run** `pnpm vitest run src/modules/products/form.test.ts` — Expected: FAIL.
- [ ] **Step 3: Implement** `form.ts`, the two pages and routes.
- [ ] **Step 4: Run** `pnpm typecheck && pnpm test && pnpm build` — PASS; manual check create → detail → edit → archive → restore against the local API, phone and desktop.
- [ ] **Step 5: Commit** — `git commit -m "web: product detail and create/edit form"`

### Task 8: Frontend — CSV import page

**Files:**
- Create: `frontend/src/modules/products/ImportPage.vue`, `frontend/public/product-import-template.csv`
- Modify: `frontend/src/app/router.ts`, `frontend/src/shared/api/client.ts` (raw-body POST)

**Interfaces:**
- Consumes: Task 5 endpoint; Task 6 `importProducts`.
- Produces:
  - `api.postRaw<T>(path, body: string, contentType: string)` (same CSRF and error handling as `post`).
  - Route `/products/import` (`products.create`). Flow: pick a `.csv` file (or download the template, header `code,name,selling_price,mrp,cost,unit,category,brand,description,aliases` plus the two example rows from `fixtures/labels`: `IP 1060-11"` 4 Section Plate 750, `IB 1290-7.8"` Easy Lunch 735) → automatic dry run → summary ("12 to add, 2 already exist, 1 problem") and a table of rows with status badges (sage `create`, neutral `skip`, muted red `error` with messages) → "Import 12 products" button, disabled while any row has an error → result message and link to the list.
- [ ] **Step 1: Write the failing test** `client.test.ts` `postRaw sends the body with its content type and CSRF header`.
- [ ] **Step 2: Run** `pnpm vitest run src/shared/api/client.test.ts` — Expected: FAIL.
- [ ] **Step 3: Implement** `postRaw`, template file, page, route.
- [ ] **Step 4: Run** `pnpm typecheck && pnpm test && pnpm build` — PASS; manual check with the template plus a file containing an error row and a duplicate.
- [ ] **Step 5: Commit** — `git commit -m "web: CSV product import with preview"`

---

## Exit criteria (Architecture.md §20, phase 2)

- `search_is_fast_at_10k_products` passes in CI (median < 100 ms).
- Typing `1060` in the Products search shows every `IP1060-*` size together, smallest size first.
- A stock operator can import a spreadsheet of products, fix errors from the preview, and import; a cashier can find and view products but not edit or see cost.
