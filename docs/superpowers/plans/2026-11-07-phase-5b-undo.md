# Phase 5b Undo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the owner cancel a mistaken sale, or take back some items later, with stock and money reversed exactly. Nothing is deleted, and the history and reports stay true.

**Architecture:**
- Two new money-moving endpoints in the `orders` backend module: `POST /orders/{id}/cancel` and `POST /orders/{id}/returns`.
- Each runs in one transaction under the existing idempotency, ledger and stock-ledger writers.
- Each locks the order's customer row, then the order row, before reading balances. That is the same lock order as later payments, so a payment can never land on an order that is being cancelled.
- Read models, the daily summary and the nightly check learn about cancelled orders and returns.
- The frontend adds Cancel and Return dialogs to the sale detail page and a "Returned" section to the receipts.

**Tech Stack:** Same as 5a (Rust/Axum/SQLx/Postgres, Vue 3/TS/Pinia/Tailwind/Vitest).

**Spec:** `docs/superpowers/specs/2026-11-06-phase-5-pos-design.md` (5b parts of §1, §2, §3, §4, §6, §8). Background: Architecture.md §10.

## Global Constraints

Everything in the 5a plan's Global Constraints still holds (`docs/superpowers/plans/2026-11-06-phase-5a-sell.md`). That covers money as strings, `parse_money`, ids via `new_id()`, ledger sign, immutability, idempotency rules, cost privacy, sqlx prepare, audit and copy. In addition:

- **Permissions.** Cancel = `orders.void`. Return = `returns.create`. Preview of a return = `returns.create`.
- **Idempotency.** `Idempotency-Key` is required on cancel and on a non-preview return. The scopes are `order_cancel:{order_id}` and `order_return:{order_id}`.
- **Locking.** Inside the transaction, lock the customer row first (`SELECT … FROM customers WHERE id = $1 FOR UPDATE`, if the order has a customer). Then lock the order row (`SELECT … FROM orders WHERE id = $1 FOR UPDATE`). Only then read owed, paid and returned quantities.
- **Ledger.**
  - New kinds: `cancel`, `refund`, `return_credit` (already allowed by the DB CHECK).
  - New account: `Account::SalesReturns` → `"sales_returns"`.
  - Ledger idempotency keys:
    - `order:{id}:cancel`
    - `order:{id}:cancel_refund:{cash|upi}`
    - `return:{return_id}:credit`
    - `return:{return_id}:refund`
- **Stock.**
  - New inbound movement types `sale_cancel` and `sale_return`, both with `unit_cost` NULL, which keeps the average cost.
  - New source type `return`.
  - Damaged returned items write no movement.
- **Reason and note.** Optional, ≤ 500 characters, trimmed; empty → NULL.
- **Copy.** Plain sentence case that says how to fix it.

## Rulings (decided while planning; the spec is silent or ambiguous)

1. **Cancel posts a sale reversal plus one refund per method.** This replaces reversing each payment transaction, as spec §2 says. A later payment can cover several sales, and reversing it whole would un-pay the other sales.
   - The `cancel` transaction: Dr `sales_revenue` T, Cr `receivable[o]` T, with `reverses_id` = the sale transaction. It is skipped when T = 0.
   - Then, for each method with money paid on this order (Σ of that method's payment credits to `receivable[o]`), a `refund` transaction: Dr `receivable[o]` P, Cr `cash`/`upi` P.
   - Net receivable is 0. The owner hands that money back.
2. **No cancel after a return.** A sale with any return can't be cancelled: 409 `HAS_RETURNS`, "Some items were already returned. Return the rest instead."
3. **The return amount is computed cumulatively per line**, so returns in several goes add up exactly to the line's `net_amount`. For q units of a line with `quantity` Q, `net_amount` N, and R units already returned:

   `round(N × (R+q) / Q) − round(N × R / Q)`, rounded half up to the paise.
4. **What's owed is reduced first, then the rest is refunded.**
   - `owed_reduced = min(amount, owed)` → a `return_credit` transaction: Dr `sales_returns`, Cr `receivable[o]`.
   - `refund = amount − owed_reduced` → a `refund` transaction: Dr `sales_returns`, Cr `cash`/`upi`.
   - `refund_method` is required only when refund > 0.
5. **Return preview.** `"preview": true` on the return request runs the same validation and maths, then rolls back and returns the amounts. It needs no idempotency key.
6. **Daily summary keeps history true.**
   - Sales figures count orders created that day whatever their status is now, so a past day never changes.
   - Undo actions are reported on the day they happen.
   - `new_owed`, `paid_on_these_sales` and `received_on_older_sales` use only `sale` and `payment` transactions.
7. **Nightly check.** A cancelled order must have owed exactly 0. Otherwise it is reported as `receivable_out_of_range`.
8. **Follow-up from 5a: persist the record-payment retry state.**
   - Save PaymentDialog's pending attempt per customer in `localStorage` (`jj.payment.v1`), so a reload after a lost response retries with the same key.
   - Clear it on success.

## Review Focus

1. **A payment racing a cancel.** A later payment and a cancel on the same customer at the same moment must never leave `receivable[o]` below 0 on the cancelled order. Expected: one waits for the other. Tested in Task 3 (`concurrent_payment_and_cancel_keep_owed_in_range`).
2. **Returning more than is left.** Return 3 when 2 were sold and 1 was already returned, or quantity 0, or a line from another order. Expected: 422 on `lines[i].quantity` / `lines[i].order_line_id`, nothing written. Tested in Task 4.
3. **Several partial returns of a discounted line must add up to exactly what was paid for it.** E.g. 3 units with net ₹100.00 returned one at a time give 33.33 + 33.34 + 33.33 = 100.00. Tested in Task 2 (`cumulative_return_amounts_sum_to_net`) and Task 4.
4. **Cancelling or returning on a ₹0 sale, or a pay-later sale with nothing paid.** There are no zero-amount entries, and nothing is refunded. Tested in Task 3 (`cancel_unpaid_sale_refunds_nothing`, `cancel_zero_total_sale`) and Task 4 (`return_on_unpaid_sale_only_reduces_owed`).
5. **Double tap on Cancel or Return, and a retry after a lost response.** Expected: one cancel or return. Tested in Task 3 and Task 4 (`retry_with_same_key_*`), and in Task 7 and Task 8 through the persisted frontend keys.

---

## File structure

**Backend:**
- Create:
  - `migrations/20261109000000_returns.sql`
  - `src/orders/undo.rs` (pure return maths)
  - `src/orders/cancel.rs`
  - `src/orders/returns.rs`
- Modify:
  - `src/ledger/mod.rs`
  - `src/inventory/domain.rs`
  - `src/orders/mod.rs`
  - `src/orders/read.rs`
  - `src/orders/daily.rs`
  - `src/inventory/check.rs`
  - `src/error.rs`
- Tests:
  - `tests/returns_schema.rs`
  - `tests/cancel.rs`
  - `tests/returns.rs`
  - extend `tests/orders_read.rs`, `tests/daily_summary.rs`, `tests/stock_check.rs`

**Frontend:**
- Create:
  - `modules/sales/CancelDialog.vue`
  - `modules/sales/ReturnDialog.vue`
  - `modules/sales/undo.ts` (+ test)
- Modify:
  - `modules/pos/api.ts`
  - `modules/sales/SaleDetailPage.vue`
  - `modules/sales/SalesPage.vue`
  - `modules/receipt/PublicReceiptPage.vue`
  - `modules/overview/DailySummaryCard.vue`
  - `modules/customers/PaymentDialog.vue` + `payment-key.ts` (+ test)

---

### Task 1: Migration — returns tables and movement types

**Files:**
- Create: `backend/migrations/20261109000000_returns.sql`
- Test: `backend/tests/returns_schema.rs`

**Interfaces:**
- Produces:
  - `returns`:
    - `id`, `order_id`, `refund_method TEXT CHECK (refund_method IN ('cash','upi'))` (nullable)
    - `amount NUMERIC(12,2) > 0`, `owed_reduced ≥ 0`, `refunded ≥ 0`
    - `CHECK (owed_reduced + refunded = amount)`
    - `CHECK ((refunded > 0) = (refund_method IS NOT NULL))`
    - `note` ≤ 500, `created_by`, `created_at`
    - index `(order_id)`
  - `return_lines`:
    - `id`, `return_id`, `order_line_id REFERENCES order_lines`
    - `quantity BIGINT > 0`, `disposition CHECK IN ('sellable','damaged')`, `amount ≥ 0`
    - index `(order_line_id)`
  - Both tables are immutable: attach the existing `ledger_immutable()` trigger function.
  - `stock_movements`:
    - the `movement_type` CHECK gains `'sale_cancel'` and `'sale_return'`
    - the sign CHECK treats both as inbound (+)
    - the `source_type` CHECK gains `'return'`

- [ ] **Step 1: Write failing tests.**
  - `return_amounts_must_add_up`: owed_reduced 10 + refunded 5 with amount 20 fails.
  - `refund_method_iff_refunded`: refunded 5 with a NULL method fails. Refunded 0 with method `'cash'` fails.
  - `return_rows_cannot_be_changed`: UPDATE on `return_lines` fails with `cannot be changed`.
  - `sale_return_movement_is_inbound`: −1 fails. +1 with source `'return'` succeeds.
- [ ] **Step 2: Run** `cargo test --test returns_schema`. Expected: FAIL (relation "returns" does not exist).
- [ ] **Step 3: Write the migration.** Look up the current CHECK names in `20261108000000_sales.sql`.
- [ ] **Step 4: Run** `cargo test --test returns_schema --test sales_schema --test inventory_schema`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "returns: schema for returns and undo stock movements"`

### Task 2: Return maths

**Files:**
- Create: `backend/src/orders/undo.rs` (register it in `orders/mod.rs`)
- Test: `#[cfg(test)]` in the same file

**Interfaces:**
- Produces:
  - `return_line_amount(net: Decimal, quantity: i64, already: i64, take: i64) -> Decimal`: Ruling 3, half up to the paise. The caller guarantees `0 < take ≤ quantity − already`.
  - `split_return(amount: Decimal, owed: Decimal) -> (Decimal /*owed_reduced*/, Decimal /*refund*/)`: Ruling 4. `owed` below 0 is treated as 0.

- [ ] **Step 1: Write failing unit tests.**
  - `cumulative_return_amounts_sum_to_net`: net 100.00, quantity 3, taken one at a time → 33.33, 33.34, 33.33, sum 100.00.
  - `whole_line_return_is_net`: net 449.99, quantity 2, take 2 → 449.99.
  - `zero_net_line_returns_zero`.
  - `split_return_reduces_owed_first`: (300, 200) → (200, 100); (150, 200) → (150, 0); (100, 0) → (0, 100).
- [ ] **Step 2: Run** `cargo test --lib orders::undo`. Expected: FAIL (does not compile).
- [ ] **Step 3: Implement.** Use integer paise, as `orders::domain` does.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: return amount and owed/refund split rules"`

### Task 3: Cancel a sale

**Files:**
- Create: `backend/src/orders/cancel.rs`
- Modify:
  - `backend/src/ledger/mod.rs`: `TxnKind::{Cancel, Refund, ReturnCredit}`, `Account::SalesReturns`, `post` accepts an optional `reverses_id: Option<Uuid>` field on `TxnInput`. Update all callers to `None`.
  - `backend/src/inventory/domain.rs`: `MovementType::{SaleCancel, SaleReturn}` (inbound), `SourceType::Return`.
  - `backend/src/orders/mod.rs` (route)
  - `backend/src/error.rs`: `has_returns()` 409 `HAS_RETURNS`, and `not_cancellable()` 409 `ALREADY_CANCELLED` "This sale is already cancelled."
- Test: `backend/tests/cancel.rs`

**Interfaces:**
- Consumes: `ledger::post`, `idempotency::{key_from_headers, hash_body, begin, finish}`, `inventory::ledger::apply`, `read::order_detail`.
- Produces:
  - `POST /orders/{id}/cancel {reason?}` with `Idempotency-Key`.
    - Response 200 is `OrderDetail` with `status: "cancelled"` and `owed: "0.00"`. `paid` becomes net of refunds in Task 5, so don't assert it here.
    - The response also carries a new field `refunded: [{method, amount}]` (empty when nothing was paid). Add it to `OrderDetail` as `refunds: Vec<RefundRow { method, amount, occurred_at }>`, filled from `refund` transactions on the order, for cancels and returns alike.
  - Transaction steps:
    - idempotency
    - lock the customer, then the order (404 if missing; 409 if cancelled; 409 `HAS_RETURNS`)
    - Ruling 1 postings
    - `sale_cancel` movements for every line, with the full quantity (source `order`, the order id)
    - `UPDATE orders SET status='cancelled', cancelled_at=now(), cancelled_by, cancel_reason`
    - audit `order.cancel`
    - finish
  - Replay rebuilds the detail, as checkout does.

- [ ] **Step 1: Write failing tests.** Use the helpers from `tests/checkout.rs`: an opening count of 10, then checkout.
  - `cancel_paid_sale_refunds_by_method_and_restores_stock`:
    - sale of 2 × ₹100 paid cash 150 + UPI 50, then cancel
    - stock is back to 10
    - `refunds` = [{cash, 150.00}, {upi, 50.00}]
    - `order_owed` = 0
    - Σ cash entries for the order's transactions nets to 0
    - status is `cancelled`
  - `cancel_part_paid_sale_after_later_payment`:
    - two pay-later sales A (₹200) and B (₹200) for one customer
    - one later payment of ₹300, so A is paid and B has 100 paid
    - cancel B → refund 100 for B's method
    - A is still paid
    - `customer_owes` = 0
  - `cancel_unpaid_sale_refunds_nothing`: no `refund` transaction, `customer_owes` back to 0.
  - `cancel_zero_total_sale`: no ledger rows written, stock restored.
  - `cancel_twice_is_409`: a second cancel with a new key → 409 `ALREADY_CANCELLED`.
  - `retry_with_same_key_cancels_once`: same key → 200 twice, one `cancel` transaction.
  - `cancel_after_return_is_409`: insert a returns row by SQL → 409 `HAS_RETURNS`.
  - `cancel_needs_orders_void`: a cashier gets 403.
  - `concurrent_payment_and_cancel_keep_owed_in_range`:
    - pay-later sale ₹200
    - run `tokio::join!` of a later payment of ₹200 and a cancel
    - whichever order they run in, the final `order_owed` = 0, no receivable is below 0, and `customer_owes` = 0
- [ ] **Step 2: Run** `cargo test --test cancel`. Expected: FAIL.
- [ ] **Step 3: Implement.** The paid amount per method is the Σ of minus the `receivable[o]` entries in `payment` transactions, joined to the transaction's method.
- [ ] **Step 4: Run** `cargo test --test cancel --test checkout --test customer_payments --test money_ledger`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: cancel a sale, refunding what was paid"`

### Task 4: Return items

**Files:**
- Create: `backend/src/orders/returns.rs`
- Modify: `backend/src/orders/mod.rs`, `backend/src/error.rs` (only if a new code is needed)
- Test: `backend/tests/returns.rs`

**Interfaces:**
- Consumes:
  - `undo::{return_line_amount, split_return}`
  - `TxnKind::{ReturnCredit, Refund}`, `Account::SalesReturns`
  - `MovementType::SaleReturn`, `SourceType::Return`
  - idempotency, `order_detail`
- Produces:
  - `POST /orders/{id}/returns`, body:

    ```json
    { "lines": [{"order_line_id": "…", "quantity": 1, "disposition": "sellable"}],
      "refund_method": "cash", "note": null, "preview": false }
    ```
  - Preview response 200: `{ amount, owed_reduced, refund, lines: [{order_line_id, quantity, amount}] }`.
  - Real response 201: `OrderDetail`, which now includes `returns` (see Task 5).
  - Validation, as field errors:
    - `lines` must not be empty.
    - Each `order_line_id` must belong to this order, and appear only once.
    - `quantity` must be from 1 to (sold − already returned). The message is "Only N left to return."
    - `disposition` must be `sellable` or `damaged`.
    - `refund_method` is required when refund > 0: "Choose cash or UPI for the refund."
  - A cancelled order → 409 `ALREADY_CANCELLED`.
  - Transaction steps:
    - idempotency, unless preview
    - lock the customer, then the order
    - compute the amounts with the already-returned totals summed from `return_lines`
    - insert `returns` and `return_lines`
    - `return_credit` and/or `refund` transactions, skipping zero amounts
    - `sale_return` movements for sellable lines (source `return`, the return id)
    - audit `order.return`
    - finish
  - Preview runs inside a transaction that is rolled back.

- [ ] **Step 1: Write failing tests.**
  - `return_sellable_item_refunds_and_restocks`: sale 2 × ₹100 paid cash 200; return 1 sellable, refund cash → refund 100.00, stock 9, `refunds` contains {cash, 100.00}.
  - `return_damaged_item_does_not_restock`: stock unchanged.
  - `return_on_unpaid_sale_only_reduces_owed`: pay-later ₹200, return 1 → owed 100.00, no `refund` transaction, `refund_method` may be omitted.
  - `return_on_part_paid_sale_reduces_owed_then_refunds`: ₹300 sale (3 × 100), paid 100, return 3 → `owed_reduced` 200, refund 100.
  - `partial_returns_add_up_to_net`: 3 units with a line discount making net 100.00; three returns of 1 → amounts 33.33, 33.34, 33.33.
  - `cannot_return_more_than_left` → 422 on `lines[0].quantity`, nothing written.
  - `line_from_another_order_is_rejected` → 422 on `lines[0].order_line_id`.
  - `refund_method_required_when_refunding` → 422 on `refund_method`.
  - `preview_writes_nothing`: preview returns the amounts; there are no `returns` rows and stock is unchanged.
  - `retry_with_same_key_returns_once`.
  - `return_on_cancelled_sale_is_409`.
  - `return_needs_returns_create` → 403 for a role without it. Check the seed; a cashier may lack it.
- [ ] **Step 2: Run** `cargo test --test returns`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test returns --test cancel --test checkout`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: return items, reducing owed then refunding"`

### Task 5: Read models, list and nightly check

**Files:**
- Modify: `backend/src/orders/read.rs`, `backend/src/inventory/check.rs`
- Test: extend `backend/tests/orders_read.rs`, `backend/tests/stock_check.rs`

**Interfaces:**
- Produces:
  - `LineOut` gains `id` (the order line id) and `returned_quantity: i64`.
  - `OrderDetail` gains:
    - `returns: Vec<ReturnRow { id, created_at, amount, owed_reduced, refunded, refund_method, note, lines: Vec<ReturnLineRow { order_line_id, product_name, quantity, disposition, amount }> }>`, newest first
    - `paid_by_method: { cash: String, upi: String }` (what's currently paid per method, net of refunds; used by the cancel dialog)
    - `refunds` (from Task 3)
  - `payment_status` is `"cancelled"` when `status == "cancelled"`.
  - `GET /orders?paid=` applies the paid filter to confirmed orders only. Cancelled orders are listed with no `paid` filter, or when `status=cancelled` is passed (new param `status`).
  - `PublicReceipt` gains `returned: Vec<{name, code, quantity, amount}>` and `refunded: String`. Its `paid` is net of refunds. `OrderDetail.paid` and `OrderSummary` paid figures are too: paid = Σ payment credits − Σ refund debits on `receivable[o]`.
  - The nightly check adds a cancelled order with owed ≠ 0 to `receivable_out_of_range` (Ruling 7).

- [ ] **Step 1: Write failing tests.**
  - `detail_after_partial_return_shows_returned_quantity_and_return_row`.
  - `detail_paid_by_method_nets_refunds`: paid cash 150 + UPI 50, return 1 of 2 with a ₹100 cash refund → `paid_by_method` cash 50.00, upi 50.00, `paid` 100.00.
  - `cancelled_detail_status_and_payment_status`: both are `cancelled`.
  - `list_paid_filter_excludes_cancelled` and `list_status_cancelled`.
  - `public_receipt_lists_returned_items`: no cost keys.
  - `stock_check_flags_cancelled_order_with_owed`: plant one by SQL.
- [ ] **Step 2: Run** `cargo test --test orders_read --test stock_check`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: show cancels and returns on sales and receipts"`

### Task 6: Daily summary with undo

**Files:**
- Modify: `backend/src/orders/daily.rs`
- Test: extend `backend/tests/daily_summary.rs`

**Interfaces:**
- Produces new response fields (Ruling 6):
  - `cancelled_count`, `cancelled_total`: orders whose `cancelled_at` falls in the day.
  - `returns_total`: Σ `returns.amount` created in the day.
  - `refunds_cash`, `refunds_upi`: Σ credits to `cash`/`upi` in `refund` transactions occurring in the day.
  - `owed_reduced`: Σ credits to `receivable` in `cancel` and `return_credit` transactions in the day, minus Σ debits to `receivable` in `refund` transactions in the day. This is the net change to what's owed from undo actions.
- Changes to existing fields:
  - `sales_count`, `sales_total` and `discounts` count every order created in the day, whatever its status.
  - `new_owed`, `paid_on_these_sales` and `received_on_older_sales` use only `sale` and `payment` transactions, and no longer filter on order status.
  - `total_owed` is unchanged.

- [ ] **Step 1: Write failing tests.**
  - `daily_summary_counts_cancel_on_the_day_it_happens`:
    - yesterday's sale ₹200 paid cash (backdate it as the existing test does)
    - cancel it today
    - yesterday's summary still shows `sales_total "200.00"` and `cash_received "200.00"`
    - today shows `cancelled_count 1`, `cancelled_total "200.00"`, `refunds_cash "200.00"`
  - `daily_summary_returns_and_refunds`:
    - today a ₹300 sale with ₹100 paid
    - return 3 with a cash refund
    - `returns_total "300.00"`, `refunds_cash "100.00"`
  - The existing `daily_summary_reconciles` still passes unchanged.
- [ ] **Step 2: Run** `cargo test --test daily_summary`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test && cargo clippy --all-targets -- -D warnings && cargo sqlx prepare --check -- --all-targets`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "reports: show cancels, returns and refunds on the day"`

### Task 7: Frontend — cancel a sale

**Files:**
- Create: `frontend/src/modules/sales/CancelDialog.vue`, `frontend/src/modules/sales/undo.ts` (+ `undo.test.ts`)
- Modify:
  - `frontend/src/modules/pos/api.ts`: types for the Task 3–6 fields, `cancelOrder(id, body, key)`, `returnItems(id, body, key?)`, and the new `DailySummary` fields
  - `frontend/src/modules/sales/SaleDetailPage.vue`
  - `frontend/src/modules/sales/SalesPage.vue`: a Cancelled filter and badge

**Interfaces:**
- Produces in `undo.ts`:
  - `pendingUndoKey(scope: string, body: unknown): string`: like the cart. It persists `{scope → {body, key}}` in `localStorage` key `jj.undo.v1` with try/catch, returns the same key for the same body after a reload, and gives a new key for a changed body.
  - `clearUndoKey(scope: string)`.
- Behaviour:
  - **Cancel sale** shows when `status` is confirmed, `returns` is empty and the user has `orders.void`.
  - The dialog shows "Hand back ₹150 cash and ₹50 UPI." from `paid_by_method`, or "Nothing was paid." It has an optional reason.
  - On confirm, call `cancelOrder` with key `pendingUndoKey('cancel:' + id, body)`. On success, clear the key and reload the detail.
  - Network errors say a retry is safe.
- [ ] **Step 1: Write failing tests** in `undo.test.ts`:
  - same body → same key
  - same body after re-import (a simulated reload with the same localStorage) → same key
  - changed body → new key
  - clear → new key
  - corrupt storage → works
- [ ] **Step 2: Run** `pnpm -C frontend test -- undo`. Expected: FAIL.
- [ ] **Step 3: Implement** the helper, the dialog and the page wiring.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px against the local API: cancel a paid sale, and see the banner and that stock is restored. Expected: PASS, with the behaviour as described.
- [ ] **Step 5: Commit** `git commit -m "sales: cancel a sale from its detail page"`

### Task 8: Frontend — return items, receipts, summary

**Files:**
- Create: `frontend/src/modules/sales/ReturnDialog.vue`
- Modify:
  - `frontend/src/modules/sales/SaleDetailPage.vue`: a Returns section and a Return items button (only with `returns.create`, when not cancelled and some quantity is left)
  - `frontend/src/modules/receipt/PublicReceiptPage.vue`: a "Returned" section, with Refunded when it is > 0
  - `frontend/src/modules/overview/DailySummaryCard.vue`: an "Undone today" group with the Task 6 fields and their how-calculated lines

**Interfaces:**
- Consumes: `returnItems`, `pendingUndoKey`/`clearUndoKey` (scope `'return:' + id`).
- Behaviour:
  - **ReturnDialog** lists each line with a quantity stepper from 0 to what's left, and a disposition toggle (Sellable / Damaged, default Sellable).
  - On any change it calls `returnItems(id, {…, preview: true})`, debounced at 300 ms, and shows "Takes ₹X off what they owe" and/or "Refund ₹Y".
  - The refund method picker (Cash / UPI) appears only when refund > 0.
  - Confirm sends the real request with the persisted key. On success it clears the key and reloads.
  - Server field errors show on the matching line.
- [ ] **Step 1: Write a failing test** in `undo.test.ts`: `returnBody(lines, method, note)` drops quantity-0 lines and omits `refund_method` when null. Put this pure helper in `undo.ts`.
- [ ] **Step 2: Run** `pnpm -C frontend test -- undo`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px: partial return with a refund, a damaged return, the public receipt in a no-cookie context, and the Overview figures. Expected: PASS, with the behaviour as described.
- [ ] **Step 5: Commit** `git commit -m "sales: return items; receipts and summary show returns"`

### Task 9: Persist the record-payment retry (5a follow-up)

**Files:**
- Modify: `frontend/src/modules/customers/payment-key.ts` (+ test), `frontend/src/modules/customers/PaymentDialog.vue`

**Interfaces:**
- Produces: `makePaymentSession(customerId)` (existing; add the parameter) persists its pending attempt `{body, key}` in `localStorage` `jj.payment.v1` keyed by customer. `succeeded()` clears it.
- [ ] **Step 1: Write a failing test.** `pending_attempt_survives_reload`: a new session for the same customer with the same storage → `pending()` returns the same body, and `key(body)` returns the same key. Another customer gets no pending attempt.
- [ ] **Step 2: Run** `pnpm -C frontend test -- payment-key`. Expected: FAIL.
- [ ] **Step 3: Implement.** Reuse `pendingUndoKey`-style storage code from `undo.ts`, or move the shared part into `shared/idempotency.ts`.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "payments: keep a pending payment across reloads"`

### Task 10: Docs

**Files:** Modify `Architecture.md` (§10.3 postings for cancel/return per Rulings 1 and 4; §10.5; §15 endpoints; §20 status), the spec's 5b notes where Ruling 1 differs (add a dated note under §2, don't rewrite).

- [ ] **Step 1: Update docs.**
- [ ] **Step 2: Run** the full checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo sqlx prepare --check -- --all-targets`, `pnpm -C frontend test && pnpm -C frontend build`. Expected: all pass.
- [ ] **Step 3: Commit** `git commit -m "docs: Phase 5b undo built"`
