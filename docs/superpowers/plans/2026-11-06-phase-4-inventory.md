# Phase 4 Inventory + Receiving Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Track stock on hand for every product in an append-only movement ledger, fed by stock counts, goods receipts and adjustments, with a weighted-average cost and a nightly consistency check.

**Architecture:**
- A new `inventory` backend module (beside `catalog`) owns all of this. One function, `ledger::apply`, is the only writer of `stock_movements` and `stock_balances`. The receipt, count and adjustment services each open a transaction, call it, then write audit rows.
- The frontend gets a `stock` module with four screens and a stock card on the existing product page.

**Tech Stack:**
- Backend: Rust, Axum, SQLx 0.9 (macros, offline `.sqlx`), Postgres, rust_decimal
- Frontend: Vue 3, TS, Tailwind v4, Vitest

**Spec:** `docs/superpowers/specs/2026-11-06-phase-4-inventory-design.md`. Background: Architecture.md §8 and §9.

## Global Constraints

- **Quantities.** Whole numbers, `BIGINT` in the database and `i64` in Rust, in the product's base unit.
- **Money.**
  - `NUMERIC(12,2)`. The API sends and receives strings such as `"120.50"`; parse input with `shared::money::parse_money`.
  - `avg_cost` is `NUMERIC(12,4)` and goes out as a string with 4 decimals.
- **Ids and errors.**
  - Ids come from `shared::ids::new_id()` (UUIDv7).
  - Errors use the standard `{error:{code,message,details,request_id}}` shape.
  - Paths use `ApiPath`, queries `ApiQuery`, JSON bodies `ApiJson`.
- **Paged lists.** `shared::paging::{Paging, Page}`: default 25, maximum 100.
- **One location.** The seeded location is `00000000-0000-7000-8002-000000000001`, code `shop`, name "Shop". The UI never shows it.
- **Permissions.** No new permission codes; see the spec's §3 table.
  - `inventory.read`: view stock, movements, stock-check results
  - `purchases.read`: view receipts and suppliers
  - `purchases.receive`: drafts, confirming, suppliers
  - `inventory.count`: counts
  - `inventory.adjust`: adjustments and a manual stock check
- **Audit.** Every confirm, post, adjustment and supplier create or edit writes `audit::record`.
- **SQLx offline data.** After any change to a `query!` call, run `cargo sqlx prepare -- --all-targets` and commit `.sqlx/`. CI runs `--check`.
- **Copy.** User-facing text is plain sentence case and says how to fix the problem, e.g. "Only 3 in stock. Enter 3 or less."
- **Stock check schedule.** 02:00 Asia/Kolkata. That zone is a fixed UTC+05:30 with no daylight saving, so use `chrono::FixedOffset::east_opt(19800)`. No new crate.

## Review Focus

1. **Product archived after it was added to a draft.** Confirm or post must refuse with 422 and name the product, not write the movement. Tested in Task 5 and Task 6.
2. **Restocking a product that has run out.** Its average is known but `on_hand` is 0. The receipt's cost becomes the new average; the old average must not be blended in. Tested in Task 3.
3. **Editing or deleting a confirmed receipt or posted count.** Must return 409 `NOT_DRAFT` and change nothing. Tested in Task 5 and Task 6.
4. **Bad numbers in line input:** quantity 0, a negative quantity, a cost with 3 decimals, or text instead of a number. Must return 422 with `details.field` set to `lines[i].quantity` or `lines[i].unit_cost`, and must never become a 500. Tested in Task 5.
5. **A first count with counted = 0.** It writes no movement, so the product still has no history and its next count is still an opening balance. It must not crash or write a zero-quantity movement. Tested in Task 6.

---

## File structure

**Backend (new `backend/src/inventory/`):**

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations, `SHOP_LOCATION_ID`, `router()` that merges the sub-routers |
| `domain.rs` | Pure rules: `next_avg_cost`, `MovementType` (with sign check), line validation |
| `ledger.rs` | `apply`, the only writer of movements and balances |
| `suppliers.rs` | Supplier DTOs, SQL and routes |
| `receipts.rs` | Goods-receipt DTOs, SQL, service and routes |
| `counts.rs` | Stock-count DTOs, SQL, service and routes |
| `stock.rs` | Stock list, product stock, movement history, adjustments |
| `check.rs` | Stock-check run, latest result, nightly scheduler |

**Backend changes elsewhere:**
- `backend/migrations/20261107000000_inventory.sql`
- `src/error.rs`: new `Coded` variant
- `src/lib.rs` and `src/http/mod.rs`: wiring
- `src/main.rs`: spawn the scheduler

**Backend tests:**
- `backend/tests/inventory_schema.rs`
- `backend/tests/inventory_ledger.rs`
- `backend/tests/suppliers.rs`
- `backend/tests/goods_receipts.rs`
- `backend/tests/stock_counts.rs`
- `backend/tests/stock.rs`
- `backend/tests/stock_check.rs`

**Frontend (new `frontend/src/modules/stock/`):**

| File | Responsibility |
|---|---|
| `api.ts` | Types and calls |
| `totals.ts` (+ test) | Line and grand totals; count differences |
| `StockPage.vue` | Stock list |
| `StockCard.vue` | Stock card used on `ProductDetailPage.vue` |
| `AdjustDialog.vue` | Adjust-stock dialog |
| `SuppliersPage.vue` | Suppliers list and form |
| `ReceiptsPage.vue`, `ReceiptEditorPage.vue` | Receive goods |
| `CountsPage.vue`, `CountEditorPage.vue` | Stock counts |
| `ProductPicker.vue` | Code search used by both editors |

**Frontend changes elsewhere:** `app/router.ts`, `app/nav.ts` (+ `nav.test.ts`), `app/layouts/NavIcon.vue`.

---

### Task 1: Migration — inventory tables, seed location, immutability

**Files:**
- Create: `backend/migrations/20261107000000_inventory.sql`
- Test: `backend/tests/inventory_schema.rs`

**Interfaces:**
- Produces:
  - The tables and columns exactly as in spec §1.
  - The trigger function `stock_movements_immutable()`, which raises `stock movements cannot be changed` on UPDATE and DELETE.
  - CHECK `stock_movements_sign`, which ties the quantity's sign to `movement_type`.
  - Allowed values:
    - `goods_receipts.status`: `'draft'`, `'confirmed'`
    - `stock_counts.status`: `'draft'`, `'posted'`
    - `source_type`: `'goods_receipt'`, `'stock_count'`, `'adjustment'`
  - Unique index `suppliers_name_active_key` on `lower(name)` where `archived_at IS NULL`.
  - `stock_balances.on_hand CHECK (on_hand >= 0)`.

- [ ] **Step 1: Write the failing tests** in `tests/inventory_schema.rs`, as `#[sqlx::test]`. Insert directly with SQL, using a product from a small helper that inserts one product.
  - `seeds_shop_location`: `SELECT code, name FROM locations WHERE id = '00000000-0000-7000-8002-000000000001'` returns `("shop","Shop")`.
  - `movements_cannot_be_updated_or_deleted`: insert one `opening_balance` +5 row. Both `UPDATE … SET quantity = 6` and `DELETE` return an error whose message contains `cannot be changed`.
  - `movement_sign_must_match_type`: inserting `purchase_receipt` with −1 fails, and so does `damage` with +1.
  - `balance_cannot_go_negative`: inserting a `stock_balances` row with on_hand −1 fails.
  - `active_supplier_names_are_unique_case_insensitive`: "Superware" and then "superware" fails. After the first is archived, inserting it again succeeds.
- [ ] **Step 2: Run** `cargo test --test inventory_schema`. Expected: FAIL (relation "locations" does not exist).
- [ ] **Step 3: Write the migration.** Add a header comment pointing to Architecture.md §8 and §9 and the spec, then the indexes:
  - `stock_movements (product_id, created_at DESC)`
  - `stock_movements (source_type, source_id)`
  - `goods_receipts (status, received_on DESC)`
  - `stock_counts (status, created_at DESC)`
- [ ] **Step 4: Run** `cargo test --test inventory_schema && cargo test --test schema`. Expected: PASS. The existing `schema.rs` still passes.
- [ ] **Step 5: Commit.** `git add backend/migrations backend/tests/inventory_schema.rs && git commit -m "inventory: schema for stock ledger, receipts, counts, suppliers"`

### Task 2: Coded errors

**Files:**
- Modify: `backend/src/error.rs`
- Test: the `#[cfg(test)] mod tests` block in `backend/src/error.rs`

**Interfaces:**
- Produces:
  - The variant:
    ```rust
    AppError::Coded {
        status: StatusCode,
        code: &'static str,
        message: String,
        details: serde_json::Value,
    }
    ```
  - It renders with the given status, code and details.
  - Helper constructors, all on `impl AppError`:
    - `fn not_draft(what: &str) -> Self`: 409 `NOT_DRAFT`, message `"This {what} is already final and can't be changed."`
    - `fn already_done(code: &'static str, message: &str) -> Self`: 409 with that code
    - `fn insufficient_stock(on_hand: i64, product: &str) -> Self`: 422 `INSUFFICIENT_STOCK`, message `"Only {on_hand} of {product} in stock. Enter {on_hand} or less."`, details `{"on_hand": on_hand}`

- [ ] **Step 1: Write the failing test** `coded_error_renders_status_code_and_details`. Build `AppError::insufficient_stock(3, "B 6143-8")`. Assert that `status()` is 422 and `code()` is `"INSUFFICIENT_STOCK"`, and that the JSON body's `error.details.on_hand` is 3. Check the body by calling `into_response()` and reading it with `axum::body::to_bytes`.
- [ ] **Step 2: Run** `cargo test --lib error`. Expected: FAIL (no variant `Coded`).
- [ ] **Step 3: Implement** the variant, its arms in `status`, `code` and `into_response`, and the three helpers.
- [ ] **Step 4: Run** `cargo test --lib error`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -am "error: coded business-rule errors"`

### Task 3: Ledger core

**Files:**
- Create: `backend/src/inventory/mod.rs`, `backend/src/inventory/domain.rs`, `backend/src/inventory/ledger.rs`
- Modify: `backend/src/lib.rs`, adding `pub mod inventory;`
- Test: unit tests in `domain.rs`; `backend/tests/inventory_ledger.rs`

**Interfaces:**
- Produces, in `inventory::domain`:
  - `pub enum MovementType { OpeningBalance, PurchaseReceipt, AdjustmentIn, AdjustmentOut, Damage }`, with `as_str()` returning the snake_case strings, `is_inbound()`, and `FromStr`.
  - `pub enum SourceType { GoodsReceipt, StockCount, Adjustment }`, with `as_str()`.
  - The cost formula:
    ```rust
    pub fn next_avg_cost(
        on_hand: i64,
        avg: Option<Decimal>,
        qty_in: i64,
        cost: Option<Decimal>,
    ) -> Option<Decimal>
    ```
- Produces, in `inventory::mod`: `pub const SHOP_LOCATION_ID: Uuid`.
- Produces, in `inventory::ledger`:
  ```rust
  pub struct MovementInput {
      pub product_id: Uuid,
      pub movement_type: MovementType,
      /// Always positive; the sign comes from movement_type.
      pub quantity: i64,
      pub unit_cost: Option<Decimal>,
      pub reason: Option<String>,
  }

  pub struct BalanceAfter {
      pub product_id: Uuid,
      pub on_hand: i64,
      pub avg_cost: Option<Decimal>,
  }

  pub struct Source {
      pub source_type: SourceType,
      pub source_id: Uuid,
  }

  /// Insert missing balance rows (0, NULL), then lock them FOR UPDATE in product_id order.
  pub async fn lock_balances(
      conn: &mut PgConnection,
      product_ids: &[Uuid],
  ) -> AppResult<HashMap<Uuid, (i64, Option<Decimal>)>>

  pub async fn apply(
      conn: &mut PgConnection,
      actor: Uuid,
      source: Source,
      moves: &[MovementInput],
  ) -> AppResult<Vec<BalanceAfter>>
  ```
  - The returned balances are in `product_id` order.
  - When an outbound move would go below zero, it returns `AppError::insufficient_stock(on_hand, <product code or name>)`.
- `next_avg_cost` rules:
  - If `cost` is `None`, return `avg`.
  - If `avg` is `None` or `on_hand <= 0`, return `cost`.
  - Otherwise return `(on_hand·avg + qty_in·cost) / (on_hand + qty_in)`, rounded to 4 decimal places with `RoundingStrategy::MidpointAwayFromZero`.
- `apply` algorithm:
  1. Sort the moves by `product_id`.
  2. Call `lock_balances` for the distinct products. Locking again in the same transaction is harmless.
  3. Fold the moves into the balance:
     - Inbound moves update avg through `next_avg_cost` (adjustment_in passes `cost: None`).
     - Outbound moves check that `on_hand >= qty`.
  4. Insert each movement with the signed quantity and `SHOP_LOCATION_ID`.
  5. `UPDATE stock_balances`.

- [ ] **Step 1: Write the failing unit tests** in `domain.rs`, as a table test called `avg_cost_cases`:

  | on_hand | avg | qty_in | cost | result |
  |---|---|---|---|---|
  | 0 | None | 10 | Some(100) | Some(100) |
  | 10 | Some(100) | 10 | Some(120) | Some(110.0000) |
  | 3 | Some(100) | 1 | Some(101) | Some(100.2500) |
  | 2 | Some(10) | 1 | Some(11) | Some(10.3333) |
  | 0 | Some(90) | 5 | Some(120) | Some(120) (**Review Focus 2**) |
  | 5 | None | 5 | Some(80) | Some(80) |
  | 5 | Some(80) | 5 | None | Some(80) |

  Also add `movement_type_round_trips`, which checks each `as_str()` and `from_str` pair.
- [ ] **Step 2: Write the failing integration tests** in `tests/inventory_ledger.rs`. They call `jj_retail_api::inventory::ledger::apply` on a transaction from the pool, with a product and user seeded by SQL helpers in the test file.
  - `inbound_creates_balance_and_movement`: opening_balance 5 at cost 100. The balance is (5, 100.0000), and there is one movement with quantity +5.
  - `outbound_stores_negative_quantity`: then adjustment_out 2. on_hand is 3, the movement quantity is −2, and avg is unchanged at 100.
  - `outbound_below_zero_is_refused_and_writes_nothing`: with on_hand 3, damage 4. The error code is `INSUFFICIENT_STOCK` with details.on_hand 3. After rollback, the movement count and on_hand are unchanged.
  - `several_products_in_one_call`: moves for two products, given in descending id order. The result is sorted ascending and both balances are correct.
  - `concurrent_outbound_never_oversells`: on_hand 1. Two tasks, each with its own transaction, run adjustment_out 1 at the same time. Exactly one commits and the other gets `INSUFFICIENT_STOCK`. on_hand is 0.
- [ ] **Step 3: Run** `cargo test --lib inventory && cargo test --test inventory_ledger`. Expected: FAIL (module not found).
- [ ] **Step 4: Implement** `mod.rs` (no router yet), `domain.rs` and `ledger.rs`.
- [ ] **Step 5: Run** the same commands. Expected: PASS. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 6: Commit.** `git add -A backend && git commit -m "inventory: ledger apply with weighted-average cost"`

### Task 4: Suppliers API

**Files:**
- Create: `backend/src/inventory/suppliers.rs`
- Modify: `backend/src/inventory/mod.rs` (`router()`), `backend/src/http/mod.rs` (`.merge(crate::inventory::router())`)
- Test: `backend/tests/suppliers.rs`

**Interfaces:**
- Produces:
  - `SupplierOut { id, name, phone: Option<String>, notes: Option<String>, archived: bool }`.
  - Routes:
    - `GET /suppliers?q=&include_archived=&page=&per_page=` returns `Page<SupplierOut>`, ordered by name.
    - `POST /suppliers {name, phone?, notes?}` returns 201 and a `SupplierOut`.
    - `GET /suppliers/{id}`
    - `PATCH /suppliers/{id} {name?, phone?, notes?, archived?}`
  - `pub(crate) async fn active_supplier_name(conn, id) -> AppResult<Option<String>>`, used by Task 5.
- Validation:
  - Name trimmed, 1–120 characters.
  - Phone optional, at most 20 characters.
  - Notes optional, at most 500 characters.
  - A duplicate active name returns `AppError::Conflict("A supplier called {name} already exists.")`.

- [ ] **Step 1: Write the failing tests:**
  - `stock_operator_creates_supplier`: 201.
  - `duplicate_active_name_conflicts`: 409.
  - `cashier_cannot_create_supplier`: 403.
  - `cashier_cannot_list_suppliers`: 403, because cashiers lack `purchases.read`.
  - `manager_archives_and_list_hides_archived`: after PATCH `{archived:true}`, the default list excludes the supplier and `include_archived=true` includes it.
  - `supplier_name_too_long_is_422`: 121 characters gives 422.
  - `supplier_edit_is_audited`: `audit_logs` has a row with action `supplier.update`.
- [ ] **Step 2: Run** `cargo test --test suppliers`. Expected: FAIL (404s).
- [ ] **Step 3: Implement.** Permissions:
  - `purchases.read` for GET
  - `purchases.receive` for POST and PATCH
  - Audit actions are `supplier.create` and `supplier.update`.
- [ ] **Step 4: Run** `cargo test --test suppliers`. Expected: PASS. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 5: Commit.** `git add -A backend && git commit -m "inventory: suppliers API"`

### Task 5: Goods receipts

**Files:**
- Create: `backend/src/inventory/receipts.rs`
- Test: `backend/tests/goods_receipts.rs`

**Interfaces:**
- Consumes:
  - `ledger::apply`, `MovementType::PurchaseReceipt`, `SourceType::GoodsReceipt`
  - `suppliers::active_supplier_name`
  - `AppError::{not_draft, already_done, FieldInvalid}`
- Produces:
  - `ReceiptLineIn { product_id: Uuid, quantity: i64, unit_cost: String }`
  - `ReceiptLineOut { product_id, code: Option<String>, name, quantity, unit_cost: Decimal, line_total: Decimal }`
  - `ReceiptOut { id, supplier: NamedRef, received_on: NaiveDate, note, status, lines: Vec<ReceiptLineOut>, total: Decimal, confirmed_at, confirmed_by_name }`
  - `ReceiptSummary { id, supplier: NamedRef, received_on, status, line_count, total }`
  - Routes:
    - `GET /goods-receipts?status=&page=` returns `Page<ReceiptSummary>`, drafts first, then `received_on DESC`.
    - `POST /goods-receipts {supplier_id, received_on?, note?}` returns 201 and a `ReceiptOut`. `received_on` defaults to today in IST.
    - `GET /goods-receipts/{id}`
    - `PATCH /goods-receipts/{id} {supplier_id?, received_on?, note?}`
    - `DELETE /goods-receipts/{id}` returns 204.
    - `PUT /goods-receipts/{id}/lines {lines: [ReceiptLineIn]}` replaces every line and returns a `ReceiptOut`.
    - `POST /goods-receipts/{id}/confirm` returns a `ReceiptOut`.
- Rules:
  - **Draft only:** PATCH, DELETE and PUT lines on a confirmed receipt return `not_draft("receipt")`.
  - **Line validation:** each line is checked in order, and the first failure returns `FieldInvalid { field: "lines[{i}].quantity" | "lines[{i}].unit_cost" | "lines[{i}].product_id", … }`.
    - Quantity 1 ..= 1_000_000.
    - `parse_money` must succeed on the cost.
    - The product must exist and not be archived.
    - No product may appear twice.
  - **Confirm**, in one transaction:
    1. `SELECT … FOR UPDATE` the receipt.
    2. Already confirmed → `already_done("ALREADY_CONFIRMED", "This receipt was already confirmed.")`.
    3. No lines → `Validation("Add at least one product before confirming.")`.
    4. Supplier archived → `Validation("{name} is archived. Pick another supplier.")`.
    5. Any line's product archived → `FieldInvalid lines[i].product_id`, message `"{code} is archived. Remove it from this receipt."` (**Review Focus 1**).
    6. `ledger::apply`.
    7. Set the status, `confirmed_by` and `confirmed_at`.
    8. Audit `goods_receipt.confirm`, with `after = {total, lines}`.
  - **Permissions:** `purchases.read` for GET, `purchases.receive` for everything else.

- [ ] **Step 1: Write the failing tests:**
  - `draft_lifecycle`: create, then PUT two lines (B 6143-8 ×10 @ "120.50", second product ×3 @ "80"). GET shows a total of "1445.00" and status draft.
  - `confirm_adds_stock_and_sets_avg_cost`: after confirm, the `stock_balances` row (read by SQL) is on_hand 10, avg_cost 120.5000. The stock endpoint arrives in Task 7.
  - `second_receipt_blends_avg_cost`: 10 @ 100, then 10 @ 120, gives avg 110.0000.
  - `confirm_twice_is_409_and_writes_nothing`: the code is `ALREADY_CONFIRMED` and the movement count is unchanged.
  - `concurrent_confirms_one_wins`: two confirms through `tokio::join!` on the same app. Statuses {200, 409}, on_hand 10.
  - `confirmed_receipt_cannot_be_edited_or_deleted`: PATCH, PUT lines and DELETE each return 409 `NOT_DRAFT` (**Review Focus 3**).
  - `bad_line_numbers_are_field_errors`, as a table (**Review Focus 4**):

    | input | `details.field` |
    |---|---|
    | quantity 0 | `lines[0].quantity` |
    | quantity -2 | `lines[0].quantity` |
    | unit_cost "12.345" | `lines[0].unit_cost` |
    | unit_cost "abc" | `lines[0].unit_cost` |
    | same product twice | `lines[1].product_id` |

  - `archived_product_blocks_confirm`: add a line, archive the product through `POST /products/{id}/archive`, then confirm. The result is 422 with `details.field` `lines[0].product_id`, and nothing is written (**Review Focus 1**).
  - `confirm_without_lines_is_422`.
  - `cashier_cannot_create_receipt`: 403.
  - `confirm_is_audited`.
- [ ] **Step 2: Run** `cargo test --test goods_receipts`. Expected: FAIL.
- [ ] **Step 3: Implement** `receipts.rs` and add its routes to `inventory::router()`.
- [ ] **Step 4: Run** `cargo test --test goods_receipts`. Expected: PASS. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 5: Commit.** `git add -A backend && git commit -m "inventory: goods receipts with confirm"`

### Task 6: Stock counts

**Files:**
- Create: `backend/src/inventory/counts.rs`
- Test: `backend/tests/stock_counts.rs`

**Interfaces:**
- Consumes: `ledger::{lock_balances, apply}`, `MovementType::{OpeningBalance, AdjustmentIn, AdjustmentOut}`, `SourceType::StockCount`.
- Produces:
  - `CountLineIn { product_id, counted: i64, unit_cost: Option<String> }`
  - `CountLineOut { product_id, code, name, counted, unit_cost: Option<Decimal>, on_hand_now: i64, expected: Option<i64>, difference: i64, changed_since_start: bool }`
    - `on_hand_now` is the current balance.
    - `expected` is filled in once the count is posted.
    - `difference` is `counted − (expected or on_hand_now)`.
    - `changed_since_start` is true when a movement for the product is newer than the count's `created_at`.
  - `CountOut { id, note, status, created_at, posted_at, lines: Vec<CountLineOut> }`
  - `CountSummary { id, note, status, created_at, line_count }`
  - Routes:
    - `GET|POST /stock-counts`
    - `GET|PATCH|DELETE /stock-counts/{id}`
    - `PUT /stock-counts/{id}/lines {lines}`
    - `POST /stock-counts/{id}/post`
- Rules:
  - Line validation follows Task 5's pattern: `lines[i].counted` must be 0 ..= 1_000_000. `unit_cost` is optional; when present `parse_money` must succeed. Products must be unique and not archived.
  - **Post**, in one transaction:
    1. Lock the count; already posted → `already_done("ALREADY_POSTED", "This count was already posted.")`.
    2. Archived products → FieldInvalid, as in Task 5.
    3. `ledger::lock_balances` for all line products. Its `on_hand` values are the `expected` values.
    4. For each line, read whether the product has any movement (`EXISTS`).
    5. Build the movements:
       - **No history:** counted > 0 → `OpeningBalance(counted, unit_cost)`; counted = 0 → nothing (**Review Focus 5**).
       - **Has history:** diff > 0 → `AdjustmentIn(diff, reason "Stock count")`; diff < 0 → `AdjustmentOut(−diff, "Stock count")`; 0 → nothing.
    6. `ledger::apply` (skip it when there are no movements). Store `expected` on each line, set the status to posted, and audit `stock_count.post` with `after = {lines: [{product_id, expected, counted}]}`.
  - Draft-only edits, as in Task 5, using `not_draft("count")`.
  - Permissions: `inventory.read` for GET, `inventory.count` for everything else.

- [ ] **Step 1: Write the failing tests:**
  - `first_count_is_opening_balance_with_cost`: counted 12 @ "95" gives one `opening_balance` +12, on_hand 12, avg 95.0000.
  - `first_count_without_cost_leaves_cost_unknown`: avg_cost IS NULL.
  - `recount_writes_adjustments`: after an opening count of 12, recount 10. That gives `adjustment_out` −2 with reason "Stock count", and expected 12 is stored.
  - `recount_up_writes_adjustment_in_and_keeps_avg`.
  - `unchanged_line_writes_no_movement`.
  - `first_count_of_zero_writes_nothing_and_next_count_is_opening` (**Review Focus 5**).
  - `changed_since_start_flags_receipt_during_count`: create the count with a line, confirm a receipt for that product, then GET the count. `changed_since_start` is true.
  - `post_twice_is_409`: code `ALREADY_POSTED`.
  - `posted_count_cannot_be_edited`: 409 `NOT_DRAFT` (**Review Focus 3**).
  - `archived_product_blocks_post` (**Review Focus 1**).
  - `cashier_cannot_post_count`: 403.
- [ ] **Step 2: Run** `cargo test --test stock_counts`. Expected: FAIL.
- [ ] **Step 3: Implement** `counts.rs` and add its routes.
- [ ] **Step 4: Run** `cargo test --test stock_counts`. Expected: PASS. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 5: Commit.** `git add -A backend && git commit -m "inventory: stock counts with opening balances"`

### Task 7: Stock list, product stock, movements, adjustments

**Files:**
- Create: `backend/src/inventory/stock.rs`
- Test: `backend/tests/stock.rs`

**Interfaces:**
- Produces:
  - `StockRow { product: ProductSummary, on_hand: i64, avg_cost: Option<Decimal> }`, reusing `catalog::search::ProductSummary`. Make its loader `pub(crate)` if it isn't already.
  - `ProductStock { on_hand: i64, avg_cost: Option<Decimal>, stock_value: Option<Decimal> }`, where `stock_value = on_hand × avg` rounded to 2 decimal places.
  - `MovementOut { id, movement_type, quantity, unit_cost, reason, source_type, source_id, created_by_name, created_at }`
  - Routes:
    - `GET /stock?q=&filter=out|unknown_cost&page=&per_page=` returns `Page<StockRow>`. Active products only.
      - With `q`, rows are ordered by the `catalog_search_hits(raw, norm)` rank, as products search is.
      - Without `q`, rows are ordered by code, then name.
      - `out`: `on_hand = 0` (including products with no balance row).
      - `unknown_cost`: `avg_cost IS NULL` and `on_hand > 0`.
    - `GET /products/{id}/stock` returns a `ProductStock`.
    - `GET /products/{id}/movements?page=` returns `Page<MovementOut>`, newest first.
    - `POST /products/{id}/adjustments {type: "adjustment_in"|"adjustment_out"|"damage", quantity, reason}` returns 201 and a `ProductStock`. The `source_id` is a new id.
- Validation for adjustments:
  - quantity 1 ..= 1_000_000
  - reason trimmed, 3–200 characters, otherwise `FieldInvalid reason`
  - `opening_balance` and `purchase_receipt` are refused with 422 `FieldInvalid type`
  - archived product → 422
  - audit action `stock.adjust`
- Permissions: GET needs `inventory.read`; POST needs `inventory.adjust`.

- [ ] **Step 1: Write the failing tests:**
  - `stock_list_shows_on_hand_and_filters`: three products, A with 5 @ cost, B with 0 and no row, C with 4 and unknown cost. `filter=out` returns [B] and `filter=unknown_cost` returns [C].
  - `stock_list_search_by_partial_code`: `q=614` finds B 6143-8 first.
  - `stock_list_is_paged`: total, page and per_page are correct for 30 products at per_page 25.
  - `damage_reduces_stock_and_is_audited`.
  - `adjust_below_zero_is_422_insufficient_stock`.
  - `adjust_needs_reason`: reason "ab" gives a 422 with field `reason`.
  - `stock_operator_cannot_adjust`: 403. Operators lack `inventory.adjust`.
  - `movements_newest_first_with_actor_name`.
  - `product_stock_for_never_stocked_product_is_zero_unknown`: `{on_hand:0, avg_cost:null, stock_value:null}`.
- [ ] **Step 2: Run** `cargo test --test stock`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test stock`. Expected: PASS. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 5: Commit.** `git add -A backend && git commit -m "inventory: stock list, history and adjustments"`

### Task 8: Stock check, nightly job, exit-criterion test

**Files:**
- Create: `backend/src/inventory/check.rs`
- Modify: `backend/src/main.rs` (in `serve`, before `axum::serve`: `tokio::spawn(jj_retail_api::inventory::check::nightly(pool.clone()))`)
- Test: `backend/tests/stock_check.rs`

**Interfaces:**
- Produces:
  - `pub async fn run(db: &PgPool) -> AppResult<CheckRun>`.
    - Compares each balance row, and each product that has movements but no balance row, against `SUM(quantity)` from `stock_movements`, for `SHOP_LOCATION_ID`.
    - Inserts a `stock_check_runs` row.
    - Logs `tracing::error!` with the mismatch count when there are any.
  - `CheckRun { id, ran_at, mismatches: Vec<Mismatch> }`
  - `Mismatch { product_id, code: Option<String>, name: String, balance: i64, ledger_sum: i64 }`. Store `{product_id, balance, ledger_sum}` in JSONB and add the code and name when reading.
  - `pub fn next_run_after(now: DateTime<Utc>) -> DateTime<Utc>` returns the next 02:00 at +05:30 strictly after `now`.
  - `pub async fn nightly(db: PgPool) -> !`. It loops forever: sleep until `next_run_after(Utc::now())`, call `run`, log an error on failure and keep going.
  - Routes:
    - `GET /stock-checks/latest` needs `inventory.read` and returns `CheckRun | null`.
    - `POST /stock-checks` needs `inventory.adjust` and returns 201 and a `CheckRun`.

- [ ] **Step 1: Write the failing tests:**
  - `next_run_after_cases`, a unit test in `check.rs`:

    | now (UTC) | next run (UTC) | why |
    |---|---|---|
    | 2026-11-06T10:00Z | 2026-11-06T20:30Z | 15:30 IST, so tonight at 02:00 IST |
    | 2026-11-06T20:30Z | 2026-11-07T20:30Z | strictly after |
    | 2026-11-06T20:29Z | 2026-11-06T20:30Z | |

  - `clean_ledger_has_no_mismatches`.
  - `planted_mismatch_is_reported_not_fixed`: `UPDATE stock_balances SET on_hand = on_hand + 1` by SQL, then POST. One mismatch with balance = sum + 1, and the balance row is still wrong afterwards.
  - `latest_returns_most_recent_run`.
  - `cashier_cannot_run_check`: 403.
  - `every_unit_traces_to_a_confirmed_source`, the **exit criterion**. Through the API:
    1. Opening count for 3 products, one with counted 0.
    2. Two confirmed receipts.
    3. A damage adjustment.
    4. A recount, plus one draft receipt left unconfirmed.

    Then assert by SQL:
    - For every balance, `on_hand = SUM(quantity)`.
    - Every movement's source exists and is final: a confirmed receipt, a posted count, or `source_type='adjustment'` with a matching `audit_logs` row whose entity_id is that source id. Record `entity_id = source_id` when auditing `stock.adjust` in Task 7. If Task 7 didn't, rule and fix it there.
    - The draft receipt has no movements.
    - `POST /stock-checks` reports 0 mismatches.
- [ ] **Step 2: Run** `cargo test --test stock_check && cargo test --lib check`. Expected: FAIL.
- [ ] **Step 3: Implement** `check.rs`, the routes and the spawn in `main.rs`.
- [ ] **Step 4: Run** the full backend suite and lint: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`. Expected: all pass. Then run `cargo sqlx prepare -- --all-targets`.
- [ ] **Step 5: Commit.** `git add -A backend && git commit -m "inventory: nightly stock check and traceability test"`

### Task 9: Frontend — API, navigation, stock list, product stock card, adjust

**Files:**
- Create:
  - `frontend/src/modules/stock/api.ts`
  - `frontend/src/modules/stock/totals.ts`
  - `frontend/src/modules/stock/totals.test.ts`
  - `frontend/src/modules/stock/StockPage.vue`
  - `frontend/src/modules/stock/StockCard.vue`
  - `frontend/src/modules/stock/AdjustDialog.vue`
- Modify:
  - `frontend/src/app/router.ts`: add the route `stock` (`inventory.read`, title "Stock")
  - `frontend/src/app/nav.ts`: add the item
  - `frontend/src/app/nav.test.ts`
  - `frontend/src/app/layouts/NavIcon.vue`: add the `stock` icon
  - `frontend/src/modules/products/ProductDetailPage.vue`: render `<StockCard>` when `can('inventory.read')`

**Interfaces:**
- Produces, in `api.ts`:
  - TS types mirroring Tasks 4–8: `Supplier`, `Receipt`, `ReceiptSummary`, `ReceiptLineIn`, `StockCount`, `CountSummary`, `CountLineIn`, `StockRow`, `ProductStock`, `Movement`, `CheckRun`.
  - One function per route:
    - `listStock`, `getProductStock`, `listMovements`, `adjustStock`
    - `listSuppliers`, `createSupplier`, `updateSupplier`
    - `listReceipts`, `createReceipt`, `getReceipt`, `updateReceipt`, `deleteReceipt`, `putReceiptLines`, `confirmReceipt`
    - `listCounts`, `createCount`, `getCount`, `updateCount`, `deleteCount`, `putCountLines`, `postCount`
    - `latestCheck`, `runCheck`
  - These use `api` from `@/shared/api/client`, following `products/api.ts`.
- Produces, in `totals.ts`:
  - `lineTotal(qty: string, cost: string): string | null`: null when either value is invalid. It uses `parseMoneyInput` and integer paise arithmetic, so there is no float error.
  - `grandTotal(lines: {quantity: string; unit_cost: string}[]): string`: invalid lines count as 0.
  - `countDifference(counted: string, onHand: number): number | null`
- Navigation:
  - `navItems` gains `{ label: 'Stock', to: '/stock', perm: 'inventory.read', icon: 'stock' }` after Products.
  - On phones the bottom tabs show at most four items. The existing `visibleNav` slice rule stays; check `AppLayout.vue` for how tabs are limited, and if there is no limit, rule on it in the ledger.

- [ ] **Step 1: Write the failing tests** in `totals.test.ts`:
  - `lineTotal('10','120.50')` is `'1205.00'`.
  - `lineTotal('3','0.10')` is `'0.30'`.
  - `lineTotal('0','5')` is null.
  - `lineTotal('2','abc')` is null.
  - `grandTotal([{quantity:'10',unit_cost:'120.50'},{quantity:'3',unit_cost:'80'}])` is `'1445.00'`.
  - `countDifference('10', 12)` is −2.
  - `countDifference('', 12)` is null.

  In `nav.test.ts`, a user with `inventory.read` sees Stock and a user without it doesn't.
- [ ] **Step 2: Run** `pnpm -C frontend test --run`. Expected: FAIL.
- [ ] **Step 3: Implement** `api.ts`, `totals.ts`, the nav item and icon, and the screens:
  - **StockPage**: the same layout as `ProductsPage.vue`, with search, paging and URL query sync through a `list-query`-style helper. Filter chips are *All*, *Out of stock* and *Cost unknown*. Rows show the code, name, on-hand quantity (`.figure`) and average cost (via `formatRupees`), or a golden `Badge` reading "Cost unknown". A danger banner shows when `latestCheck()` has mismatches: "Stock check found {n} mismatches. Tell the admin." Admins also get a **Run check** button.
  - **StockCard**: on-hand quantity, average cost and stock value; an **Adjust stock** button when the person has `inventory.adjust`; movement history in pages of 10. Each history entry shows a type label, the signed quantity, the reason, who made it and when. Type labels:
    - opening_balance: Opening count
    - purchase_receipt: Received
    - adjustment_in: Added
    - adjustment_out: Removed
    - damage: Damaged
  - **AdjustDialog**: a native `<dialog>` with a type radio (Add / Remove / Damaged), quantity and reason. It shows the API error under its field using `details.field`. After saving it emits `saved` with the new `ProductStock`.
- [ ] **Step 4: Run** `pnpm -C frontend test --run && pnpm -C frontend typecheck && pnpm -C frontend build`. Expected: all pass.
- [ ] **Step 5: Look at it.** Run the app locally (backend plus `pnpm dev`) and screenshot the stock list and the product page in light and dark at 390 px and 1280 px. Fix anything off-system: tokens only, no raw colours.
- [ ] **Step 6: Commit.** `git add -A frontend && git commit -m "stock: list, product stock card, adjustments"`

### Task 10: Frontend — suppliers and receive goods

**Files:**
- Create:
  - `frontend/src/modules/stock/SuppliersPage.vue`
  - `frontend/src/modules/stock/ReceiptsPage.vue`
  - `frontend/src/modules/stock/ReceiptEditorPage.vue`
  - `frontend/src/modules/stock/ProductPicker.vue`
- Modify: `frontend/src/app/router.ts`

**Interfaces:**
- Consumes: `api.ts` and `totals.ts` from Task 9; `searchProducts` from `products/api.ts`.
- Produces:
  - Routes:
    - `stock/suppliers`: `purchases.read`, "Suppliers"
    - `stock/receipts`: `purchases.read`, "Receive goods"
    - `stock/receipts/:id`: `purchases.read`, "Receipt"
  - `ProductPicker` props and events: `{ exclude: string[] }`, emits `pick(ProductSummary)`. It searches with a 250 ms debounce, needs at least 1 character, and offers up to 8 results, labelled code first, then name.
- Screens:
  - **SuppliersPage**: a paged list with an Add/Edit form inline, plus an Archive or Restore action.
  - **ReceiptsPage**: drafts first, with a **New receipt** button that asks for a supplier (pick, or *Add supplier* inline), then opens the editor.
  - **ReceiptEditorPage**:
    - A header with the supplier, date and note.
    - A line table with product, quantity, cost (pre-filled from the product's `default_cost` when set) and line total. Lines are removable, and new ones come from `ProductPicker`.
    - The grand total.
    - **Save draft** (PUT lines) and **Confirm receipt** (saves, then a `<dialog>`: "Add these {n} products to stock? This can't be undone.", then confirm).
    - An API `details.field` of `lines[i].x` highlights that row's cell and scrolls to it.
    - After confirming, the page is read-only and shows the confirmed time and who confirmed it.
    - On a phone, lines stack as cards; on a desktop they are a table.
    - Only people with `purchases.receive` see edit controls.

- [ ] **Step 1: Write the failing test.** Extract `fieldToLineIndex(field: string): {index: number; key: string} | null` into `totals.ts`:
  - `'lines[2].unit_cost'` → `{index:2,key:'unit_cost'}`
  - `'name'` → null
- [ ] **Step 2: Run** `pnpm -C frontend test --run`. Expected: FAIL.
- [ ] **Step 3: Implement** the helper and the screens.
- [ ] **Step 4: Run** `pnpm -C frontend test --run && pnpm -C frontend typecheck && pnpm -C frontend build`. Expected: pass.
- [ ] **Step 5: Look at it.** Run the full flow locally: add a supplier, create a receipt, add two lines, cause an error (cost "12.345"), fix it and confirm. The stock list then shows the new quantities. Screenshot at 390 px and 1280 px.
- [ ] **Step 6: Commit.** `git add -A frontend && git commit -m "stock: suppliers and receive goods"`

### Task 11: Frontend — stock counts

**Files:**
- Create: `frontend/src/modules/stock/CountsPage.vue`, `frontend/src/modules/stock/CountEditorPage.vue`
- Modify: `frontend/src/app/router.ts`

**Interfaces:**
- Consumes: `api.ts`, `countDifference`, `ProductPicker`, `fieldToLineIndex`.
- Produces: routes `stock/counts` (`inventory.read`, "Stock counts") and `stock/counts/:id` (`inventory.read`, "Stock count").
- Screens:
  - **CountsPage**: a list with **New count**, which takes an optional note.
  - **CountEditorPage**:
    - Lines show product, counted quantity and optional cost. The cost field shows only when `on_hand_now == 0` and the product has no history, labelled "Cost each (if known)".
    - **Save** (PUT lines).
    - **Review** shows each line's expected, counted and difference, with differences in sage when up and danger when down. Rows with `changed_since_start` get a golden badge: "Stock changed since you started. Recount this one."
    - **Post count** opens a confirm `<dialog>`, then posts.
    - After posting, the page is read-only and shows the stored `expected`.
    - Only people with `inventory.count` see edit controls.
- Also add a **Stock counts** link and a **Receive goods** link at the top of StockPage, shown by permission. The nav stays at one Stock item.

- [ ] **Step 1: Write the failing test** in `totals.test.ts`: `reviewTone(diff: number): 'sage'|'danger'|'neutral'`. Positive → sage, negative → danger, 0 → neutral.
- [ ] **Step 2: Run** `pnpm -C frontend test --run`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm -C frontend test --run && pnpm -C frontend typecheck && pnpm -C frontend build`. Expected: pass.
- [ ] **Step 5: Look at it.** Locally: an opening count of two products (one with cost, one without), post it, check the stock list (one shows "Cost unknown"), recount one lower, post it, then check the product's history. Screenshot.
- [ ] **Step 6: Commit.** `git add -A frontend && git commit -m "stock: stock counts"`

### Task 12: Docs and full verification

**Files:**
- Modify:
  - `Architecture.md` §20: add "Phase 4 built 6 Nov 2026" under the roadmap table
  - `Operations.md`: a go-live step, "Do the opening stock count (Stock → Stock counts) before the first sale", and a note that the nightly stock check runs at 02:00 IST and that its result is shown on the Stock page

- [ ] **Step 1: Run everything:**
  ```
  cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && SQLX_OFFLINE=false cargo sqlx prepare --check -- --all-targets
  pnpm -C frontend lint && pnpm -C frontend test --run && pnpm -C frontend typecheck && pnpm -C frontend build
  ```
  Expected: all pass. If `lint` isn't a script, skip it and note that in the ledger.
- [ ] **Step 2: Edit the docs.**
- [ ] **Step 3: Commit.** `git commit -am "docs: Phase 4 inventory built; go-live opening count"`
