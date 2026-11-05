# Phase 5a Sell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record a sale on the phone in one tap. The sale takes stock off, posts money to a double-entry ledger, supports pay later and later part payments, and hands the owner a ready WhatsApp receipt with a link to a public receipt page.

**Architecture:**
- Three new backend modules beside `inventory`:
  - `ledger`: the only writer of `ledger_transactions` and `ledger_entries`; zero-sum is enforced in the database.
  - `customers`: phone-keyed customers and their later payments.
  - `orders`: checkout, document numbers, receipts, WhatsApp text and the daily summary.
- Checkout is one transaction. It calls `inventory::ledger::apply` for stock and `ledger::post` for money.
- Retries are made safe by a generic `idempotency_keys` table in `shared::idempotency`.
- Frontend: new `pos`, `sales`, `customers` and `receipt` modules. The cart is a Pinia store kept in `localStorage`.

**Tech Stack:**
- Backend: Rust, Axum 0.8, SQLx 0.9 (macros, offline `.sqlx`), Postgres, rust_decimal, sha2, getrandom, base64
- Frontend: Vue 3, TS, Pinia, Tailwind v4, Vitest

**Spec:** `docs/superpowers/specs/2026-11-06-phase-5-pos-design.md` (5a only; cancel and returns are 5b). Background: Architecture.md §10, §14, §15.

## Global Constraints

- **Money.**
  - `NUMERIC(12,2)`, sent as strings (`"120.50"`).
  - Parse input with `shared::money::parse_money`; format with `format_rupees`.
  - Frontend maths uses whole paise (BigInt), as in `modules/stock/totals.ts`.
- **Quantities.** `BIGINT` / `i64`, from 1 to 1,000,000.
- **Ids and errors.**
  - Ids come from `shared::ids::new_id()`.
  - Errors use the standard shape. Use `AppError::Coded` for new codes and `FieldInvalid` for per-field problems (`lines[i].unit_price`, `customer.phone`, `payments[i].amount`).
- **Ledger sign.** Debit is positive, credit is negative. Every transaction sums to 0, and no entry has amount 0.
- **Accounts.** `receivable` (ref = order id), `cash`, `upi`, `sales_revenue`.
- **Ledger and stock rows are never updated or deleted.** Database triggers enforce this.
- **Numbers.**
  - Format `JJ/<fy>/<6-digit seq>`, e.g. `JJ/2026-27/000042`.
  - The fiscal year starts 1 April in Asia/Kolkata (`inventory::ist()`).
  - `doc_type` = `receipt`.
- **Phone.** Stored as exactly 10 digits, first digit 6–9. Accept `+91`, `91`, a leading `0`, spaces and dashes.
- **Idempotency.** The `Idempotency-Key` header (8–100 characters) is **required** on `POST /orders/checkout` and `POST /customers/{id}/payments`.
  - Missing → 400 `IDEMPOTENCY_KEY_REQUIRED`.
  - Same key + same body → the stored response, with status 200.
  - Same key + different body → 409 `IDEMPOTENCY_CONFLICT`.
- **Permissions.** No new codes.

  | Action | Permission |
  |---|---|
  | Checkout | `orders.confirm` |
  | View orders and customers | `orders.read` |
  | Create or edit customers | `orders.confirm` |
  | Later payments | `payments.record` |
  | Daily summary | `reports.read` |
  | Public receipt | none |
- **Public URL.** New config `public_url` (env `PUBLIC_URL`, default `https://jehovah-jire.hnamtek.in`, no trailing slash). The receipt link is `{public_url}/r/{receipt_token}`.
- **Shop name.** Read from the `settings` key `shop_name` (a JSON string), seeded `"Jehovah Jireh"`.
- **Cost privacy.** `unit_cost` and `avg_cost` never appear in a public receipt. In order JSON they appear only when the user has `products.update` (same rule as commit 57fce5d).
- **SQLx offline data.** After any change to a `query!`, run `cargo sqlx prepare -- --all-targets` with `.superpowers/tools/bin/sqlx` and commit `.sqlx/`.
- **Audit.** Checkout, customer create/edit and payments write `audit::record`.
- **Copy.** Plain sentence case that says how to fix it, e.g. "Paid is more than the total. Enter ₹1,200.00 or less."
- **No Playwright.** The repo has no Playwright setup; the spec's end-to-end test is done as a manual phone check in Task 14 and noted for Phase 7.

## Review Focus

1. **A ₹0 sale** (a free item, or a discount equal to the subtotal). There is nothing to post, and an entry of 0 would break the `amount <> 0` check. Expected: the sale is saved and numbered, no `sale` ledger transaction is written, and it shows as "Paid in full". Tested in Task 6 (`zero_total_sale_posts_no_money`).
2. **Paid more than owed.** This means cash + UPI over the total at checkout, or a later payment above what the customer owes. Expected: 422 `OVERPAYMENT`, nothing written. Tested in Task 6 and Task 8.
3. **Phone typed in different ways** (`+91 98620-12345`, `09862012345`, `919862012345`) must reach the same customer. A 9-digit, a 5-starting or a landline-length number must give a field error on `customer.phone`, never a 500. Tested in Task 2 and Task 5.
4. **Cart edited after a failed charge.** A different body sent with the old key would get 409. The frontend must make a new key whenever the cart, customer or payment changes, and reuse it only for an identical retry. Tested in Task 10 (`key_changes_when_cart_changes`).
5. **Sale at the fiscal-year boundary.** 31 Mar 23:59 IST is `2025-26`; 1 Apr 00:00 IST (31 Mar 18:30 UTC) is `2026-27`, and that year's numbering starts at 000001. Tested in Task 2 (`fiscal_year_boundary`) and Task 6 (`numbering_restarts_each_fiscal_year`).

---

## File structure

**Backend, new:**

| File | Responsibility |
|---|---|
| `migrations/20261108000000_sales.sql` | All 5a tables, triggers, check extensions, `shop_name` seed |
| `src/shared/phone.rs` | `normalize_phone` |
| `src/shared/idempotency.rs` | `begin` / `finish` over `idempotency_keys` |
| `src/shared/rate_limit.rs` | Fixed-window per-key limiter |
| `src/ledger/mod.rs` | `Account`, `TxnInput`, `post`, balance queries |
| `src/customers/mod.rs` | Router; `dto.rs`, `repo.rs`, `payments.rs` |
| `src/orders/mod.rs` | Router |
| `src/orders/domain.rs` | Pure rules: line pricing, discount spreading, fiscal year, payment allocation |
| `src/orders/numbers.rs` | `next_number` |
| `src/orders/checkout.rs` | Checkout service and handler |
| `src/orders/read.rs` | Order detail/list, public receipt |
| `src/orders/whatsapp.rs` | Receipt and reminder text, `wa.me` link |
| `src/orders/daily.rs` | Daily summary |

**Backend, modified:**
- `src/lib.rs`, `src/http/mod.rs` (routers)
- `src/state.rs` (receipt limiter)
- `src/config.rs` (`public_url`), `tests/common/mod.rs` (`test_config`)
- `src/inventory/domain.rs` (`Sale`, `SourceType::Order`)
- `src/inventory/check.rs` (money check)
- `src/error.rs` (constructors)
- `deploy/env.production.example`

**Backend tests:** `tests/sales_schema.rs`, `tests/money_ledger.rs`, `tests/idempotency.rs`, `tests/customers.rs`, `tests/checkout.rs`, `tests/orders_read.rs`, `tests/customer_payments.rs`, `tests/daily_summary.rs`.

**Frontend, new:**
- `modules/pos/`
  - `api.ts`
  - `cart.ts` (+ test): pure cart maths
  - `cart.store.ts` (+ test)
  - `PosPage.vue`, `CustomerPicker.vue`, `ChargeSheet.vue`, `SaleDone.vue`
- `modules/sales/`: `SalesPage.vue`, `SaleDetailPage.vue`
- `modules/customers/`: `CustomersPage.vue`, `CustomerDetailPage.vue`, `PaymentDialog.vue`
- `modules/receipt/`: `PublicReceiptPage.vue`
- `modules/overview/DailySummaryCard.vue`
- `shared/whatsapp.ts` (+ test)

**Frontend, modified:** `app/router.ts`, `app/nav.ts` (+ test), `app/layouts/NavIcon.vue`, `modules/overview/OverviewPage.vue`.

---

### Task 1: Migration — sales, money and idempotency tables

**Files:**
- Create: `backend/migrations/20261108000000_sales.sql`
- Test: `backend/tests/sales_schema.rs`

**Interfaces:**
- Produces these tables (columns as in spec §1, plus):
  - `customers`: `phone TEXT NOT NULL UNIQUE CHECK (phone ~ '^[6-9][0-9]{9}$')`, `name` ≤ 120, `note` ≤ 500, `created_by`.
  - `document_sequences (doc_type TEXT, fiscal_year TEXT, next_value BIGINT NOT NULL DEFAULT 1, PRIMARY KEY (doc_type, fiscal_year))`.
  - `orders`:
    - `status CHECK IN ('confirmed','cancelled')`, `fulfilment CHECK IN ('counter')`
    - `CHECK (total = subtotal - order_discount AND total >= 0 AND order_discount >= 0)`
    - `receipt_token TEXT UNIQUE NOT NULL`
    - the 5b cancel columns, nullable
    - index `(created_at DESC)`, index `(customer_id, created_at)`
  - `order_lines`:
    - the spec §1 columns, `UNIQUE (order_id, line_no)`
    - `CHECK (net_amount = quantity * unit_price - line_discount - order_discount_share AND net_amount >= 0)`
  - `ledger_transactions`: Architecture.md §14 columns, `kind CHECK IN ('sale','payment','cancel','refund','return_credit')`, `method CHECK IN ('cash','upi','internal')`, `customer_id UUID REFERENCES customers`, `note`, and index `(occurred_at)`.
  - `ledger_entries`:
    - `account CHECK IN ('receivable','cash','upi','sales_revenue','sales_returns')`
    - `amount <> 0`
    - `CHECK ((account = 'receivable') = (ref_id IS NOT NULL))`
    - index `(account, ref_id)`
  - `idempotency_keys`: `(key TEXT, user_id UUID, scope TEXT, request_hash TEXT NOT NULL, response JSONB, created_at, PRIMARY KEY (user_id, scope, key))`.
- Produces these triggers and seeds:
  - Constraint trigger `ledger_balanced` (`DEFERRABLE INITIALLY DEFERRED`, AFTER INSERT on `ledger_entries`). It raises `ledger transaction does not balance` when the transaction's sum ≠ 0.
  - Triggers `ledger_immutable` on both ledger tables and on `order_lines`. They raise `… cannot be changed` on UPDATE/DELETE.
  - `stock_movements` checks are recreated: `movement_type` gains `'sale'` (outbound sign) and `source_type` gains `'order'`. `sale_cancel` / `sale_return` wait for 5b.
  - `INSERT INTO settings (key, value) VALUES ('shop_name', '"Jehovah Jireh"') ON CONFLICT DO NOTHING`.

- [ ] **Step 1: Write failing tests** (`#[sqlx::test]`, raw SQL, with users/products from `common::seed_user`/`seed_product`):
  - `unbalanced_ledger_transaction_is_rejected_at_commit`: in one tx, insert a transaction with entries +100 / −90. `COMMIT` returns an error containing `does not balance`.
  - `balanced_ledger_transaction_commits`: +100 / −100 commits.
  - `ledger_rows_cannot_be_changed`: UPDATE and DELETE on an entry both fail with `cannot be changed`.
  - `receivable_needs_an_order_ref`: a `receivable` entry with NULL `ref_id` fails.
  - `customer_phone_must_be_ten_digits`: `'98620'` fails; `'9862012345'` succeeds; a second `'9862012345'` fails (unique).
  - `order_total_must_match`: subtotal 100, discount 10, total 95 fails.
  - `sale_movement_must_be_negative`: a `sale` movement with +1 fails; −1 with `source_type 'order'` succeeds.
  - `shop_name_is_seeded`: `SELECT value #>> '{}' FROM settings WHERE key='shop_name'` returns `Jehovah Jireh`.
- [ ] **Step 2: Run** `cargo test --test sales_schema`. Expected: FAIL (relation "customers" does not exist).
- [ ] **Step 3: Write the migration.** Header comment points to the spec and ADR-0004/0006. Recreate the two `stock_movements` CHECKs with `ALTER TABLE … DROP CONSTRAINT … ADD CONSTRAINT`; look up their current names in `20261107000000_inventory.sql`.
- [ ] **Step 4: Run** `cargo test --test sales_schema --test inventory_schema --test schema`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "sales: schema for customers, orders, money ledger, idempotency"`

### Task 2: Pure rules — phone, fiscal year, pricing, allocation

**Files:**
- Create: `backend/src/shared/phone.rs`, `backend/src/orders/domain.rs`, `backend/src/orders/mod.rs` (only `pub mod domain;` for now)
- Modify: `backend/src/shared/mod.rs`, `backend/src/lib.rs`
- Test: `#[cfg(test)]` blocks in each file

**Interfaces:**
- Produces:
  - `shared::phone::normalize_phone(s: &str) -> Result<String, String>`
  - `orders::domain::fiscal_year(at: DateTime<Utc>) -> String`: `"2026-27"`
  - `orders::domain::format_number(fy: &str, seq: i64) -> String`: `"JJ/2026-27/000042"`
  - `struct LineIn { quantity: i64, list_price: Decimal, unit_price: Decimal, line_discount: Decimal }`
  - `struct PricedLine { gross: Decimal /* qty×unit_price */, line_discount: Decimal, order_discount_share: Decimal, net_amount: Decimal }`
  - `price_lines(lines: &[LineIn], order_discount: Decimal) -> Result<(Vec<PricedLine>, Decimal /*subtotal*/, Decimal /*total*/), PriceError>`
  - `enum PriceError { LineDiscountTooBig(usize), OrderDiscountTooBig { max: Decimal } }`
  - `allocate(amount: Decimal, owed: &[(Uuid, Decimal)]) -> Vec<(Uuid, Decimal)>`: `owed` is oldest first; it fills each in turn and skips zero shares.

- [ ] **Step 1: Write failing unit tests**
  - `normalizes_indian_mobiles`: `"+91 98620-12345"`, `"09862012345"`, `"919862012345"` and `"9862012345"` all give `"9862012345"`.
  - `rejects_bad_phones`: `"98620123"`, `"5862012345"`, `"98620123456"`, `"abc"` and `""` are all `Err`.
  - `fiscal_year_boundary`: `2026-03-31T18:29:59Z` → `"2025-26"`; `2026-03-31T18:30:00Z` → `"2026-27"`; `2027-01-15T00:00:00Z` → `"2026-27"`.
  - `format_number_pads_to_six`: `format_number("2026-27", 42)` → `"JJ/2026-27/000042"`.
  - `price_lines_spreads_order_discount_exactly`. Lines: 1 × ₹100, 1 × ₹200, 1 × ₹300 (subtotal 600); order discount ₹100. The shares are 16.67 / 33.33 / 50.00 and sum to exactly 100.00. The total is 500.00, and each `net_amount = gross − line_discount − share`.
  - `price_lines_remainder_goes_to_largest_fraction`. Three lines of ₹100 with a ₹10 discount: the shares are 3.34 / 3.33 / 3.33 and sum to 10.00. A tie goes to the earliest line.
  - `price_lines_line_discount_applies_before_spreading`. 2 × ₹500 with a line discount of 100 → that line's base is 900. Then an order discount is spread over the line bases (gross − line_discount).
  - `price_lines_rejects_discounts_larger_than_amount`. A line discount of 101 on a ₹100 line → `LineDiscountTooBig(0)`. An order discount of 601 on subtotal 600 → `OrderDiscountTooBig { max: 600 }`.
  - `price_lines_full_discount_gives_zero_total`. Order discount = subtotal → total 0.00, every net 0.00.
  - `allocate_fills_oldest_first`. Paying 450 against `[(a,300),(b,300)]` gives `[(a,300),(b,150)]`. Paying 300 gives `[(a,300)]`.
- [ ] **Step 2: Run** `cargo test --lib orders::domain shared::phone`. Expected: FAIL (does not compile).
- [ ] **Step 3: Implement.**
  - Subtotal = Σ gross. "Line base" = gross − line_discount.
  - Spread: share_i = floor(discount × base_i / Σbase) to the paise. The leftover paise go one each to the lines with the largest truncated remainder, ties to the lowest index.
  - If Σbase = 0, the order discount must be 0.
  - The fiscal year uses `inventory::ist()`: months ≥ 4 → `Y-(Y+1)%100`, otherwise `(Y-1)-Y%100`, both two-digit padded.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "sales: phone, fiscal year, pricing and allocation rules"`

### Task 3: Money ledger writer

**Files:**
- Create: `backend/src/ledger/mod.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/money_ledger.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum Account { Receivable(Uuid), Cash, Upi, SalesRevenue }
  pub enum TxnKind { Sale, Payment }          // 5b adds the rest
  pub enum Method { Cash, Upi, Internal }
  pub struct TxnInput<'a> {
      pub kind: TxnKind, pub order_id: Option<Uuid>, pub customer_id: Option<Uuid>,
      pub method: Method, pub actor: Uuid, pub idempotency_key: String,
      pub note: Option<&'a str>, pub entries: Vec<(Account, Decimal)>,
  }
  pub async fn post(conn: &mut PgConnection, t: TxnInput<'_>) -> AppResult<Uuid>;
  pub async fn order_owed(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Decimal>;
  pub async fn orders_owed(conn: &mut PgConnection, ids: &[Uuid]) -> AppResult<HashMap<Uuid, Decimal>>;
  /// Unpaid orders of a customer, oldest first, with what's owed on each (> 0 only).
  pub async fn customer_open_orders(conn: &mut PgConnection, customer_id: Uuid) -> AppResult<Vec<(Uuid, Decimal)>>;
  pub async fn customer_owes(conn: &mut PgConnection, customer_id: Uuid) -> AppResult<Decimal>;
  ```
- `post` checks before writing: entries non-empty, no zero amount, sum = 0. Otherwise it returns `AppError::Internal`, because that's a programming error. The `idempotency_key` passed is the request key plus a suffix (`:sale`, `:payment`), so it's unique per transaction.

- [ ] **Step 1: Write failing tests**
  - `post_writes_entries_and_owed_is_their_sum`. Post a sale: receivable(o) +1000 / revenue −1000. Then a payment: cash +600 / receivable(o) −600. `order_owed` = 400.00.
  - `post_refuses_unbalanced_entries_in_code`. +100 / −90 gives an `Err` and writes no row.
  - `customer_open_orders_is_oldest_first_and_skips_paid`. Three orders for one customer: owed 300, 0 and 200. Result: two orders, oldest first.
  - Each test inserts orders with a small SQL helper `seed_order(pool, customer, total, created_at)` local to this test file.
- [ ] **Step 2: Run** `cargo test --test money_ledger`. Expected: FAIL.
- [ ] **Step 3: Implement.** Account maps to `(account TEXT, ref_id)`. Ordering is by `orders.created_at, orders.id`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "ledger: double-entry writer and balance queries"`

### Task 4: Idempotency keys

**Files:**
- Create: `backend/src/shared/idempotency.rs`
- Modify: `backend/src/shared/mod.rs`, `backend/src/error.rs` (constructors `idempotency_key_required()`, `idempotency_conflict()`)
- Test: `backend/tests/idempotency.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn key_from_headers(h: &HeaderMap) -> AppResult<String>;        // 400 IDEMPOTENCY_KEY_REQUIRED
  pub fn hash_body(v: &serde_json::Value) -> String;                  // sha256 hex of v.to_string()
  pub enum Begin { Fresh, Replay(serde_json::Value) }
  /// Inside the caller's transaction. INSERT … ON CONFLICT DO NOTHING; on conflict, read the row
  /// (this blocks until a concurrent holder commits): same hash + response → Replay; different
  /// hash → 409 IDEMPOTENCY_CONFLICT.
  pub async fn begin(conn: &mut PgConnection, user: Uuid, scope: &str, key: &str, hash: &str) -> AppResult<Begin>;
  pub async fn finish(conn: &mut PgConnection, user: Uuid, scope: &str, key: &str, response: &serde_json::Value) -> AppResult<()>;
  ```
- Handlers parse the body as `serde_json::Value` first (for the hash), then deserialize the typed request from it.

- [ ] **Step 1: Write failing tests** (call the functions directly against the pool):
  - `fresh_then_replay`: `begin` → Fresh; `finish(v)`; commit. Then `begin` with the same hash → `Replay(v)`.
  - `different_body_conflicts`: same key, different hash → error code `IDEMPOTENCY_CONFLICT`, status 409.
  - `rolled_back_key_is_free_again`: `begin`, then roll back. `begin` again → Fresh.
  - `keys_are_per_user`: user A's key used by user B → Fresh.
  - `missing_or_short_header_is_400`: `key_from_headers` with no header, or with `"abc"` → 400 `IDEMPOTENCY_KEY_REQUIRED`.
- [ ] **Step 2: Run** `cargo test --test idempotency`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "shared: idempotency keys for money-moving requests"`

### Task 5: Customers API

**Files:**
- Create: `backend/src/customers/{mod.rs,dto.rs,repo.rs}`
- Modify: `backend/src/lib.rs`, `backend/src/http/mod.rs`
- Test: `backend/tests/customers.rs`

**Interfaces:**
- Consumes: `normalize_phone`, `ledger::customer_owes`.
- Produces:
  - `POST /customers {phone, name?, note?}` → 201 `Customer`. A duplicate phone → 409 `DUPLICATE` with `details.customer_id`.
  - `GET /customers?q=&page=&per_page=` → `Page<CustomerRow>`.
    - When `q` is all digits/`+`/space it matches a phone prefix, after stripping a leading `+91`/`0`.
    - Otherwise it matches name, case-insensitive `ILIKE '%q%'`.
    - Ordered by most recent sale, then name.
  - `GET /customers/{id}` → `CustomerDetail { customer, owes, orders: Vec<OrderSummary> /* latest 50 */, payments: Vec<PaymentRow> /* latest 50 */ }`.
  - `PATCH /customers/{id} {phone?, name?, note?}` → `Customer`.
  - `pub async fn find_or_create(conn, actor, phone: &str, name: Option<&str>) -> AppResult<Uuid>`, used by checkout. If the phone exists, it returns that customer and fills in the name only when it is empty.
  - `struct Customer { id, phone, name: Option<String>, note: Option<String>, created_at }`
  - `struct CustomerRow { customer: Customer, owes: String, last_sale_at: Option<DateTime<Utc>> }`
  - `OrderSummary { id, number, created_at, total, owed, status }` is defined in `orders::read` (Task 7). Until then, Task 5 returns `orders: []`, and Task 7 fills it.

- [ ] **Step 1: Write failing tests** (logged in as a `cashier` for reads/creates; a user with no roles for 403):
  - `create_normalizes_phone`: POST `"+91 98620-12345"` → `phone == "9862012345"`.
  - `bad_phone_is_field_error`: POST `"12345"` → 422, `details.field == "phone"`.
  - `duplicate_phone_conflicts`: POST twice → 409 `DUPLICATE`, `details.customer_id` = the first id.
  - `search_by_phone_prefix_and_name`: seed "Mary" 9862012345 and "John" 9436000000. `q=98620` → [Mary]; `q=0986` → [Mary]; `q=joh` → [John].
  - `detail_shows_zero_owes_for_new_customer`: `owes == "0.00"`.
  - `no_permission_is_403`.
- [ ] **Step 2: Run** `cargo test --test customers`. Expected: FAIL.
- [ ] **Step 3: Implement** following `inventory/suppliers.rs` for DTO, repo and routes. Audit `customer.create` / `customer.update`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "customers: phone-keyed customers API"`

### Task 6: Checkout

**Files:**
- Create: `backend/src/orders/numbers.rs`, `backend/src/orders/checkout.rs`
- Modify:
  - `backend/src/orders/mod.rs` (router)
  - `backend/src/inventory/domain.rs`: `MovementType::Sale` (outbound, `"sale"`), `SourceType::Order` (`"order"`)
  - `backend/src/error.rs`: `overpayment(max: Decimal)`, `customer_required()`
  - `backend/src/http/mod.rs`
- Test: `backend/tests/checkout.rs`

**Interfaces:**
- Consumes: `price_lines`, `fiscal_year`, `format_number`, `inventory::ledger::apply`, `inventory::check_line_products`, `ledger::post`, `idempotency::{begin, finish}`, `customers::repo::find_or_create`.
- Produces:
  - `numbers::next_number(conn, doc_type: &str, at: DateTime<Utc>) -> AppResult<String>`: `INSERT … ON CONFLICT (doc_type, fiscal_year) DO UPDATE SET next_value = document_sequences.next_value + 1 RETURNING next_value - 1`. Seed the row with `next_value = 2` on first insert so the first number is 1.
  - `POST /orders/checkout`, with this body:
    ```json
    { "lines": [{"product_id": "…", "quantity": 2, "unit_price": "450.00", "line_discount": "0"}],
      "order_discount": "50", "customer_id": null,
      "new_customer": {"phone": "9862012345", "name": "Mary"},
      "payments": [{"method": "cash", "amount": "800"}], "note": null }
    ```
    - `unit_price` and `line_discount` are optional: they default to the product's `selling_price` and 0.
    - `customer_id` and `new_customer` can't both be given.
    - `payments`: at most one per method, each amount > 0.
  - Response 201 (200 on replay) is `OrderDetail` (defined in Task 7; until Task 7 return `{ "id", "number", "total", "paid", "owed" }`, which Task 7 extends).
  - Error codes:
    - `OVERPAYMENT` 422 (`details.max`)
    - `CUSTOMER_REQUIRED` 422 (paid < total and no customer)
    - `INSUFFICIENT_STOCK` 422
    - `FieldInvalid` for `lines[i].quantity|unit_price|line_discount|product_id`, `order_discount`, `payments[i].amount|method`, `new_customer.phone`
    - an empty cart → `FieldInvalid lines` "Add at least one item."
- Transaction order (spec §3): idempotency `begin` → validate and price → customer → `next_number` → `inventory::ledger::apply(Source{Order, order_id}, Sale moves)` → insert order and lines (snapshot `unit_cost` = `avg_cost` from the `BalanceAfter` that `apply` returns; an outbound move never changes the average) → `ledger::post` the sale (skipped when total = 0) → `ledger::post` the payment (skipped when paid = 0) → audit `order.checkout` → `finish` → commit.
- `receipt_token`: 16 random bytes from `getrandom`, base64 URL-safe without padding.

- [ ] **Step 1: Write failing tests** (cashier session; products seeded with an opening count of 10 via the stock-count API, as in `tests/stock.rs`; helper `checkout(a, s, key, body) -> TestResponse`):
  - `cash_sale_takes_stock_posts_money_and_numbers`. 2 × product (₹100), cash 200 → 201, `number == "JJ/<current fy>/000001"`, `owed == "0.00"`. Stock on hand is 8. The ledger has a `sale` and a `payment` transaction, and `order_owed` = 0.
  - `server_prices_ignore_client_totals`. The body carries `"total": "1"` and no `unit_price` → total 200.00.
  - `price_edit_and_discounts_are_snapshotted`. `unit_price` 90 and line discount 10 on 2 units, order discount 20 → line `list_price 100.00`, `unit_price 90.00`, `net_amount 150.00`, total 150.00.
  - `pay_later_needs_a_customer`. No payment and no customer → 422 `CUSTOMER_REQUIRED`. With `new_customer` → 201, `owed == total`, and the customer exists.
  - `part_payment_leaves_owed`. Cash 50 + UPI 50 on a ₹200 sale → `paid "100.00"`, `owed "100.00"`. The ledger has `cash` +50 and `upi` +50.
  - `overpayment_is_refused`. Cash 250 on a ₹200 sale → 422 `OVERPAYMENT`. No order and no stock change.
  - `zero_total_sale_posts_no_money`. Order discount = subtotal → 201, total 0.00, no `ledger_transactions` for the order, and stock is deducted.
  - `insufficient_stock_writes_nothing`. Quantity 11 → 422 `INSUFFICIENT_STOCK`. No order rows, and `document_sequences.next_value` is unchanged (the next sale gets 000001).
  - `retry_with_same_key_makes_one_sale`. The same key and body twice → the second is 200 with the same `id`. Stock is 8, not 6.
  - `same_key_different_body_conflicts` → 409 `IDEMPOTENCY_CONFLICT`.
  - `missing_key_is_400`.
  - `concurrent_checkouts_for_last_unit_one_wins`. Stock 1; two different keys via `tokio::join!` → one 201 and one 422.
  - `concurrent_checkouts_get_gapless_numbers`. Stock 10; five concurrent checkouts → numbers 000001–000005, each once.
  - `numbering_restarts_each_fiscal_year`. Call `numbers::next_number` directly with `at` in FY 2025-26, twice, then in FY 2026-27 → `…/2025-26/000001`, `…/000002`, `…/2026-27/000001`.
  - `existing_phone_reuses_customer`. `new_customer` with a phone that already exists → the same `customer_id`, no 409.
  - `cashier_without_confirm_is_403`. The user has only `orders.read`.
- [ ] **Step 2: Run** `cargo test --test checkout`. Expected: FAIL.
- [ ] **Step 3: Implement** `numbers.rs`, then `checkout.rs`. Map `PriceError` to the field errors above.
- [ ] **Step 4: Run** `cargo test --test checkout --test stock --test inventory_ledger`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: one-call checkout with stock, ledger and numbering"`

### Task 7: Order reads, WhatsApp text, public receipt

**Files:**
- Create: `backend/src/orders/read.rs`, `backend/src/orders/whatsapp.rs`, `backend/src/shared/rate_limit.rs`
- Modify:
  - `backend/src/config.rs` (`public_url`), `backend/tests/common/mod.rs` (`public_url: "https://shop.test".into()`)
  - `backend/src/state.rs` (`receipt_limiter: Arc<RateLimiter>`)
  - `backend/src/orders/checkout.rs` (returns `OrderDetail`), `backend/src/customers/repo.rs` (fills `orders`)
  - `deploy/env.production.example` (`PUBLIC_URL=`)
- Test: `backend/tests/orders_read.rs`, unit tests in `whatsapp.rs`

**Interfaces:**
- Produces:
  - `struct OrderDetail { id, number, status, created_at, customer: Option<Customer>, lines: Vec<LineOut>, subtotal, order_discount, total, paid, owed, payment_status: "paid"|"partly_paid"|"unpaid", payments: Vec<PaymentRow>, note, receipt_url, whatsapp_url: Option<String> }`.
  - `LineOut { product_id, product_name, product_code, quantity, list_price, unit_price, line_discount, order_discount_share, net_amount, unit_cost: Option<String> /* only with products.update */ }`.
  - `PaymentRow { id, method, amount, occurred_at }`. The amount is the cash/upi debit; this is a later-payment share when it's a multi-order transaction.
  - `pub async fn order_detail(conn, id, show_cost: bool, cfg: &Config) -> AppResult<OrderDetail>`.
  - `GET /orders/{id}` → `OrderDetail`.
  - `GET /orders?from=&to=&paid=paid|partly_paid|unpaid&customer_id=&page=` → `Page<OrderSummary>`. Dates are IST calendar dates and inclusive. Newest first.
  - `GET /public/receipts/{token}` → `PublicReceipt { shop_name, number, created_at, customer_name: Option<String>, lines: [{name, code, quantity, unit_price, list_price, net_amount}], subtotal, order_discount, total, paid, owed, cancelled: bool }`.
    - No auth. Unknown token → 404.
    - Limited to 60 requests/min per client IP (`ClientMeta`); over the limit → 429 `RATE_LIMITED`.
  - `whatsapp::receipt_text(r: &PublicReceipt, url: &str, payment_methods: &[&str]) -> String`.
  - `whatsapp::wa_link(phone10: &str, text: &str) -> String` = `https://wa.me/91{phone}?text={percent-encoded}`. Encode everything except unreserved characters (`A-Za-z0-9-_.~`), written inline, no new crate.
  - `rate_limit::RateLimiter::new(max: usize, window: Duration)`, `.check(key: &str) -> bool`.

- [ ] **Step 1: Write failing tests**
  - Unit `receipt_text_matches_spec`: two lines (`2 × IP 1060-11"` ₹900, `1 × B 6143-8 MP4952` ₹350), discount 50, total 1200, paid 800 cash. The result equals the spec §5 block exactly (date formatted `6 Nov 2026`, amounts via `format_rupees` without `.00` when whole, url appended).
    - Fix the exact template from spec §5 as constants: the header line `*{shop}* — Receipt {number}`, line rows `{qty} × {code or name} — {amount}`, then `Discount — −{amount}` (only if > 0), `*Total {amount}*`, and the paid line.
    - The paid line is `Paid in full ({methods})` when owed = 0, `Paid {paid} ({methods}) · *Due {owed}*` when partly paid, and `*Due {owed}*` when unpaid.
  - Unit `receipt_text_truncates_after_15_lines`: 17 lines → 15 rows, then `+2 more items — see link`.
  - Unit `wa_link_encodes`: `wa_link("9862012345", "a b*\n₹")` → `https://wa.me/919862012345?text=a%20b%2A%0A%E2%82%B9`.
  - `detail_shows_lines_paid_owed_and_links`. After a part-paid checkout with a customer → `payment_status == "partly_paid"`, `receipt_url` starts with `https://shop.test/r/`, and `whatsapp_url` starts with `https://wa.me/91`.
  - `detail_hides_cost_without_products_update` (cashier → `unit_cost` absent or null; admin → present).
  - `no_whatsapp_url_without_customer`.
  - `list_filters_by_paid_status_and_date`.
  - `public_receipt_by_token_without_login`. GET without a cookie → 200 with `shop_name == "Jehovah Jireh"`. The body contains no `unit_cost` or `avg_cost` key anywhere.
  - `public_receipt_unknown_token_404`.
  - `public_receipt_rate_limited`. Build an app with a limiter of max 2 and make 3 GETs → the third is 429. Add `common::app_with_state` if needed.
  - `checkout_response_is_order_detail`. It has `lines` and `whatsapp_url`.
- [ ] **Step 2: Run** `cargo test --test orders_read && cargo test --lib orders::whatsapp`. Expected: FAIL.
- [ ] **Step 3: Implement.** `money_short(d)` in `whatsapp.rs` prints `₹1,200` for whole amounts and `₹1,200.50` otherwise. It reuses `format_rupees` and strips `.00`.
- [ ] **Step 4: Run** the same command plus `cargo test --test checkout --test customers`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: order detail, list, public receipt and WhatsApp text"`

### Task 8: Later payments and reminders

**Files:**
- Create: `backend/src/customers/payments.rs`
- Modify: `backend/src/customers/mod.rs`, `backend/src/orders/whatsapp.rs`, `backend/src/customers/repo.rs` (detail gets `reminder_url`)
- Test: `backend/tests/customer_payments.rs`, a unit test in `whatsapp.rs`

**Interfaces:**
- Consumes: `ledger::{customer_open_orders, post}`, `orders::domain::allocate`, `idempotency`.
- Produces:
  - `POST /customers/{id}/payments {method: "cash"|"upi", amount, note?}`, with the `Idempotency-Key` header.
    - Response 201: `{ transaction_id, applied: [{order_id, number, amount}], owes }`.
    - One `payment` transaction: Dr cash/upi `amount`, and Cr `receivable[o]` per allocated share. `customer_id` is set.
    - `amount > owes` → 422 `OVERPAYMENT` (`details.max`). Owes 0 → the same error with max 0.00.
    - Lock the customer row `FOR UPDATE` first, so two concurrent payments can't both pass the check.
  - `CustomerDetail.reminder_url: Option<String>`, present when owes > 0.
  - `whatsapp::reminder_text(shop, name: Option<&str>, owes: Decimal, numbers: &[String]) -> String` = `Hello {name or "there"}, you have {owes} due with {shop} for receipts {numbers joined ", "}. Thank you!`

- [ ] **Step 1: Write failing tests**
  - `payment_applies_oldest_first`. Two unpaid sales of ₹200 for one customer; pay 300 UPI → applied `[(first,200),(second,100)]` and `owes "100.00"`. The first order's `payment_status` is `paid`, the second's `partly_paid`.
  - `payment_over_owes_is_refused` → 422 `OVERPAYMENT`, `details.max == "400.00"`.
  - `payment_retry_is_idempotent` → the same `transaction_id`, and owes drops only once.
  - `concurrent_payments_cannot_overpay`. Owes 200; two concurrent payments of 150 with different keys → one 201, one 422.
  - `payment_needs_payments_record`. A user with only `orders.read` → 403.
  - Unit `reminder_text_lists_receipts`.
- [ ] **Step 2: Run** `cargo test --test customer_payments`. Expected: FAIL.
- [ ] **Step 3: Implement.** Audit `customer.payment`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "customers: record later payments, oldest sale first"`

### Task 9: Daily summary and nightly money check

**Files:**
- Create: `backend/src/orders/daily.rs`
- Modify: `backend/src/orders/mod.rs`, `backend/src/inventory/check.rs`
- Test: `backend/tests/daily_summary.rs`, a new case in `backend/tests/stock_check.rs`

**Interfaces:**
- Produces:
  - `GET /reports/daily?date=YYYY-MM-DD` (default today IST), with this response:
    ```json
    { "date": "2026-11-06", "sales_count": 3, "sales_total": "…", "discounts": "…",
      "cash_received": "…", "upi_received": "…", "new_owed": "…", "total_owed": "…" }
    ```
    Definitions, for the IST day `[d 00:00, d+1 00:00)`:
    - `sales_count` / `sales_total`: confirmed orders created in the day.
    - `discounts` = Σ(qty × list_price) − Σ total over those orders.
    - `cash_received` / `upi_received`: Σ positive `cash` / `upi` entries of `payment` transactions occurring in the day, including later payments for older sales.
    - `new_owed` = Σ `receivable` entries on the day's orders whose transaction occurred in the day.
    - `total_owed` = Σ all `receivable` entries.
  - `check::run` also records money mismatches in the same `stock_check_runs.mismatches` JSON array, as `{ "kind": "ledger_unbalanced", "transaction_id" }` and `{ "kind": "receivable_out_of_range", "order_id", "owed" }` (owed < 0 or > total). Existing stock rows get `"kind": "stock"`; keep reading old rows without `kind` as stock.

- [ ] **Step 1: Write failing tests**
  - `daily_summary_reconciles`. Seed one pay-later sale of ₹100 and backdate it (SQL: set `orders.created_at` and its ledger transactions' `occurred_at` to yesterday). Today, make three sales: ₹200 all cash, ₹300 with ₹150 UPI, and ₹100 pay-later. Also pay ₹50 cash on yesterday's sale through the API. Expect:
    - `sales_count 3`, `sales_total "600.00"`
    - `cash_received "250.00"`, `upi_received "150.00"`
    - `new_owed "250.00"`, so 600 = (200 + 150) + 250 for today's sales
    - `total_owed "300.00"` (250 + 50 left on yesterday's)
  - `daily_summary_other_date_is_empty`. `date=2020-01-01` → zeros, `"0.00"`.
  - `daily_summary_needs_reports_read` → 403 for a cashier without it.
  - `stock_check_flags_receivable_out_of_range`. Insert a balanced transaction by SQL that over-credits an order (owed −10) → a run reports `receivable_out_of_range`.
- [ ] **Step 2: Run** `cargo test --test daily_summary --test stock_check`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full backend suite: `cargo test && cargo clippy --all-targets -- -D warnings && cargo sqlx prepare --check -- --all-targets`. Expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "reports: daily sales summary; nightly money check"`

### Task 10: Frontend — POS API types, cart maths, cart store

**Files:**
- Create: `frontend/src/modules/pos/api.ts`, `cart.ts`, `cart.test.ts`, `cart.store.ts`, `cart.store.test.ts`, `frontend/src/shared/whatsapp.ts` (+ test)
- Test: the two test files

**Interfaces:**
- Produces:
  - `api.ts`:
    - types that mirror `OrderDetail`, `OrderSummary`, `Customer`, `CustomerRow`, `CustomerDetail`, `DailySummary`, `PublicReceipt`, `CheckoutBody`
    - `checkout(body, key)`, `getOrder(id)`, `listOrders(q)`, `searchCustomers(q, signal)`, `createCustomer(b)`, `getCustomer(id)`, `updateCustomer(id, b)`, `recordPayment(id, b, key)`, `dailySummary(date?)`, `publicReceipt(token)`
    - Calls with a key send header `Idempotency-Key`. Extend `shared/api/client.ts` `RequestOptions` with `headers?: Record<string,string>`.
  - `cart.ts` (pure, paise BigInt):
    - `interface CartLine { productId, code, name, listPrice: string, unitPrice: string, quantity: number, lineDiscount: string }`
    - `lineNet(l) -> string`
    - `cartTotals(lines, orderDiscount: string) -> { subtotal, discount, total }`
    - `percentOff(amount: string, pct: number) -> string`: rounds to the paise, half up.
    - `roundDownTo(total: string, target: string) -> string`: the order discount that makes the total equal `target`; null if target > total.
  - `cart.store.ts` (Pinia `useCartStore`):
    - state `lines`, `orderDiscount`, `customer: Customer | null`, `newCustomer: {phone,name} | null`
    - `add(hit)` (+1 if the product is already present), `setQty`, `setPrice`, `setLineDiscount`, `remove`, `clear`
    - `chargeKey(): string`: returns the stored key while the body (`JSON.stringify(toBody())`) is unchanged, and a new `crypto.randomUUID()` when it changed
    - `toBody(payments) -> CheckoutBody`
    - Persisted to `localStorage` key `jj.cart.v1`, read in a try/catch.
  - `shared/whatsapp.ts`: `openWhatsApp(url: string)` → `window.location.href = url`. The server builds the URL; this just centralises it for mocking.

- [ ] **Step 1: Write failing tests**
  - `cart.test.ts`:
    - `lineNet` of 2 × 450.00, discount 10 → `"890.00"`
    - `cartTotals` with order discount 50 → total = subtotal − 50
    - `percentOff("999.00", 10)` → `"99.90"`
    - `roundDownTo("1043.50","1000")` → `"43.50"`
    - `roundDownTo("900","1000")` → null
  - `cart.store.test.ts`:
    - `add_same_product_increments`
    - `persists_and_restores_from_localStorage`
    - `corrupt_localStorage_starts_empty`
    - `key_changes_when_cart_changes`: `chargeKey()` twice returns the same key; after `setQty` it returns a different key; after reverting the change it returns a new key, not the old one (keys are never reused once the body changes)
- [ ] **Step 2: Run** `pnpm -C frontend test -- pos`. Expected: FAIL.
- [ ] **Step 3: Implement.** Reuse `toPaise`/`fromPaise`: move them from `modules/stock/totals.ts` to `shared/money.ts` as exports and update the import in totals.ts.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "pos: cart maths, cart store and API client"`

### Task 11: Frontend — POS screen

**Files:**
- Create: `frontend/src/modules/pos/PosPage.vue`, `CustomerPicker.vue`, `ChargeSheet.vue`, `SaleDone.vue`
- Modify: `frontend/src/app/router.ts` (`/pos`, perm `orders.confirm`), `app/nav.ts` + `nav.test.ts` (POS first after Overview, icon `pos`; Sales `/sales` perm `orders.read`, icon `sales`; Customers `/customers` perm `orders.read`, icon `customers`), `app/layouts/NavIcon.vue` (three icons)

**Interfaces:**
- Consumes: `useCartStore`, `searchProducts` (products api), `searchCustomers`, `createCustomer`, `checkout`, `openWhatsApp`, `formatRupees`, `parseMoneyInput`.
- Behaviour (spec §4):
  - **Search box:** 120 ms debounce with `AbortController`. `/` focuses it; `Enter` adds the top hit; tapping a hit adds it.
  - **Cart line:** −/+ buttons and a tap-to-edit price (the list price is struck through when changed). "Off" accepts `₹` or `%`.
  - **Whole-sale discount:** ₹ off, or "Round to ₹…" using `roundDownTo`.
  - **CustomerPicker:** debounced search; when there's no match, an "Add 98620 12345" row with an optional name field. The picked customer shows "owes ₹X" when owes > 0.
  - **ChargeSheet:**
    - cash and UPI fields; default cash = total; a "Pay later" button sets both to 0
    - shows "Left owing ₹X"
    - Charge is disabled while owing > 0 with no customer, with the hint "Pick a customer to leave money owing."
    - `F2` opens it
    - Charge calls `checkout(store.toBody(payments), store.chargeKey())`
    - Errors show inline; `INSUFFICIENT_STOCK` highlights the line by `product_id` when the details allow, otherwise it shows the message
    - The cart is kept on error
  - **SaleDone:** number, total, paid, owed; a **Send on WhatsApp** button (only when `whatsapp_url`) calls `openWhatsApp`; **New sale** clears the cart.
- [ ] **Step 1: Write failing test** in `nav.test.ts`: `visibleNav` for a user with `orders.confirm` + `orders.read` includes POS, Sales and Customers in that order after Overview; a user with only `inventory.read` sees none of them.
- [ ] **Step 2: Run** `pnpm -C frontend test -- nav`. Expected: FAIL.
- [ ] **Step 3: Implement** the nav, router and components. Mobile first at 360 px width, with tap targets ≥ 44 px.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend build`. Then `vite preview` against a local API: do a 2-item cash sale, a part-paid sale with a new customer, and an over-stock error. Expected: tests and build pass, and the three flows behave as described.
- [ ] **Step 5: Commit** `git commit -m "pos: sale screen, customer picker, charge sheet"`

### Task 12: Frontend — Sales and Customers screens

**Files:**
- Create: `frontend/src/modules/sales/SalesPage.vue`, `SaleDetailPage.vue`, `frontend/src/modules/customers/CustomersPage.vue`, `CustomerDetailPage.vue`, `PaymentDialog.vue`
- Modify: `frontend/src/app/router.ts` (`/sales`, `/sales/:id`, `/customers`, `/customers/:id`)

**Interfaces:**
- Consumes: `listOrders`, `getOrder`, `searchCustomers`, `getCustomer`, `updateCustomer`, `recordPayment`, `openWhatsApp`.
- Behaviour:
  - **Sales list:** a date filter (default today), a Paid / Partly paid / Unpaid filter, and pages of 25. Each row shows number, time, customer, total and an owed badge.
  - **Sale detail:** shows the receipt layout, **Send on WhatsApp**, and **Record payment** (when owed > 0 and a customer exists; it opens `PaymentDialog` for that customer).
  - **Customers list:** search, with owes shown.
  - **Customer detail:** edit name/phone/note, owes, sales, payments, **Record payment**, and **Remind on WhatsApp** (when `reminder_url`).
  - **PaymentDialog:** method (cash/UPI), amount (defaults to owes), note.
    - The key is generated when the dialog opens, and regenerated when the amount or method changes.
    - On success it shows "Applied to JJ/…/000042 ₹200, …" and reloads the parent.
  - Buttons are hidden without the permission (`payments.record` for Record payment).
- [ ] **Step 1: Write a failing test** `PaymentDialog` key behaviour as a pure helper `paymentKey(prev, body)` in `modules/customers/payment-key.ts` (+ test): same body → same key, changed body → a new key.
- [ ] **Step 2: Run** `pnpm -C frontend test -- payment-key`. Expected: FAIL.
- [ ] **Step 3: Implement** the helper and the screens. Reuse the cart store's key logic by moving it to `shared/idempotency.ts` (`makeKeyed()` returning `key(body)`) and use that in both places.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend build`, then do a manual preview: record a later payment and see owes drop. Expected: pass, and it behaves as described.
- [ ] **Step 5: Commit** `git commit -m "sales, customers: lists, details, record payment, reminders"`

### Task 13: Frontend — public receipt page and daily summary

**Files:**
- Create: `frontend/src/modules/receipt/PublicReceiptPage.vue`, `frontend/src/modules/overview/DailySummaryCard.vue`
- Modify: `frontend/src/app/router.ts` (top-level `/r/:token`, `meta: { public: true, bare: true }`, outside `AppLayout`), `frontend/src/app/guards.ts` + `guards.test.ts`, `frontend/src/modules/overview/OverviewPage.vue`

**Interfaces:**
- Consumes: `publicReceipt(token)`, `dailySummary(date)`.
- Behaviour:
  - **Receipt page:** no app chrome and no login. Shows the shop name, number, date, lines (list price struck through when it differs), discount, total, paid, owed, and a red "CANCELLED" banner when `cancelled`.
    - Unknown token: the page says "Receipt not found. Check the link you were sent."
    - It must render for a visitor with no session, without calling `/auth/me` redirects.
  - **Summary card:** on Overview for users with `reports.read`. Shows today's figures with labels and a small "how it's calculated" line each (spec §6); a date picker loads other days.
- [ ] **Step 1: Write a failing test** in `guards.test.ts`: `/r/abc` with no session is allowed and not redirected to `/login`.
- [ ] **Step 2: Run** `pnpm -C frontend test -- guards`. Expected: PASS is possible because `meta.public` already exists. Either way, the test stays as a pin on this behaviour.
- [ ] **Step 3: Implement.** Make sure the session store doesn't trigger `onUnauthenticated` navigation on the bare page.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend build`, then preview: open a receipt link in a private window. Expected: pass, and the page renders without login.
- [ ] **Step 5: Commit** `git commit -m "receipt: public receipt page; overview daily summary"`

### Task 14: Docs, deploy notes, phone check

**Files:**
- Modify: `Architecture.md` (§10.1, §10.3 account names, §10.6, §10.7, §14 tables, §15 endpoints, §20 status, §21 answered questions, per the spec's "Changes to Architecture.md"), `docs/adr/0005-unified-order-model.md` (append a dated note on status `completed` being derived; don't rewrite), `deploy/README.md` (add `PUBLIC_URL`)

- [ ] **Step 1: Update the docs** as listed. Note under §19 that end-to-end Playwright is still to be added in Phase 7.
- [ ] **Step 2: Run** the full checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo sqlx prepare --check -- --all-targets` (in `backend/`), then `pnpm -C frontend test && pnpm -C frontend build`. Expected: all pass.
- [ ] **Step 3: Manual phone check** (after the owner deploys). Do a cash sale → Send on WhatsApp opens WhatsApp with the text → the link opens the receipt. Then a pay-later sale → record a payment from the customer page → the Overview totals add up.
- [ ] **Step 4: Commit** `git commit -m "docs: Phase 5a selling built"`
