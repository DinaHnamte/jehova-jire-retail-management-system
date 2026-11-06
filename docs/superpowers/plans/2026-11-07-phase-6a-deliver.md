# Phase 6a Deliver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sell an order for delivery, assign it to a driver who logs in on their own phone, and track it to delivered, failed-and-retried, returned or cancelled, with stock and the delivery fee exactly right.

**Architecture:**
- A new `deliveries` backend module owns the delivery row, its state machine, the owner endpoints and the driver `/me` endpoints.
- A small `drivers` module owns driver accounts.
- Checkout gains an optional `delivery` block and posts the fee to `delivery_fee_income`.
- The 5b cancel is split into a lock-free core that the delivery cancel and check-in reuse inside their own transactions.
- The frontend adds Delivery to the Charge sheet, a Deliveries section for the owner, a separate `/driver` area, and driver management on the Users page.

**Tech Stack:** Same as 5a/5b (Rust/Axum/SQLx/Postgres, Vue 3/TS/Pinia/Tailwind/Vitest).

**Spec:** `docs/superpowers/specs/2026-11-07-phase-6a-deliver-design.md`. Background: `Delivery.md`, Architecture.md §10.

## Global Constraints

Everything in the 5a and 5b plans' Global Constraints still holds: money as strings with `parse_money`, ids via `new_id()`, ledger sign (debit +), immutability, idempotency rules, cost privacy, `cargo sqlx prepare`, audit on every write, and plain sentence-case copy that says how to fix the problem. In addition:

- **Permissions** (all already seeded; admin has every one):
  - `deliveries.read` for lists and detail
  - `deliveries.assign` for assign, edit, retry and status on a driver's behalf
  - `deliveries.cancel`, `deliveries.check_in`
  - `deliveries.read_own`, `deliveries.update_own` for drivers
  - `drivers.manage`
- **Lock order** for anything that touches money or stock: customer row, then order row, then delivery row (`FOR UPDATE`). Status-only actions lock only the delivery row.
- **Idempotency.** `Idempotency-Key` is required on checkout (as now), `PATCH /deliveries/{id}`, cancel and check-in. The scopes are:
  - `delivery_edit:{delivery_id}`
  - `order_cancel:{order_id}` (cancel reuses the order cancel)
  - `delivery_check_in:{delivery_id}`
- **Ledger.**
  - New account `Account::DeliveryFeeIncome` → `"delivery_fee_income"`.
  - New kind `TxnKind::FeeAdjustment` → `"fee_adjustment"`.
  - Ledger idempotency keys: `order:{id}:sale` (unchanged), `delivery:{id}:fee:{event_id}`.
- **Stock.** New `SourceType::Delivery` → `"delivery"`, used for the `damage` movements written at check-in.
- **Phones.** Recipient and driver phones go through `shared::phone::normalize_phone`, so `+91 98620 00000` is stored as `9862000000`.
- **Text limits** are as in spec §2.3. Inputs are trimmed, and empty optional text becomes NULL.
- **Copy.** Statuses on screen:

  | Code | Shown as |
  |---|---|
  | `to_send` | To send |
  | `assigned` | Assigned |
  | `picked_up` | Out for delivery |
  | `delivered` | Delivered |
  | `failed` | Couldn't deliver |
  | `returned` | Returned to shop |
  | `cancelled` | Cancelled |

  Failed reasons:

  | Code | Shown as |
  |---|---|
  | `customer_unavailable` | Customer not there |
  | `address_not_found` | Couldn't find the address |
  | `customer_refused` | Customer refused |
  | `unreachable_by_phone` | Customer didn't answer the phone |
  | `other` | Other |

## Rulings (decided while planning; the spec is silent or the code forces them)

1. **Current total.** What a delivery order comes to is `orders.total − orders.delivery_fee + deliveries.fee`. Write it as one SQL expression via `LEFT JOIN deliveries`; it equals `orders.total` for counter sales. It is used by `OrderDetail.total`, `OrderSummary.total`, the public receipt, the nightly check's upper bound and the daily summary's `cancelled_total`.
2. **"Returned" payment status** compares returns with the **goods** total (`orders.total − orders.delivery_fee`), not the total. Returns never refund the fee, so a fully returned delivery order would otherwise never show "returned".
3. **Cancel core.** `orders::cancel` is split into:
   - `lock_order(tx, order_id) -> LockedOrder` (customer, then order)
   - `cancel_core(tx, actor, &LockedOrder, reason) -> Vec<Value /*refunds*/>`, which assumes the locks are held and the status checks are done.

   The `cancel` transaction's entries become:
   - Dr `sales_revenue` goods total
   - Dr `delivery_fee_income` current fee
   - Cr `receivable[o]` current total

   Zero entries are skipped, and no transaction is written when the current total is 0.
4. **Cancelling a delivery order.**
   - Through either `POST /orders/{id}/cancel` or `POST /deliveries/{id}/cancel`, it is allowed only while the delivery is `to_send` or `assigned`. It also sets the delivery `cancelled`, `closed_at`, ends the open assignment (`cancelled`) and writes a status event.
   - Otherwise it is 409 `DELIVERY_OUT`: "This delivery is out with the driver. Mark it delivered or couldn't deliver first."
   - `POST /deliveries/{id}/cancel` looks up the order id and calls `orders::cancel::cancel` (same scope, same response), after checking `deliveries.cancel`.
5. **Returns on a delivery order** are allowed only when the delivery is `delivered`. Otherwise they are 409 `DELIVERY_NOT_DONE`: "Items can be returned once the delivery is done."
6. **Fee edit.**
   - The difference `new − old` posts a `fee_adjustment` transaction: Dr `receivable[o]` / Cr `delivery_fee_income` for an increase, the reverse for a decrease.
   - If the order's owed would go below 0, it is a 422 field error on `fee`: "The customer has already paid more than that. Cancel the delivery or return items to give money back."
   - A PATCH that changes no money still needs the key, so the frontend always sends one.
7. **Driver pay at assignment.**
   - `driver_pay` in the request wins.
   - Otherwise the delivery's existing `driver_pay` is kept.
   - Otherwise the driver's `default_pay` is used.
8. **Status errors.** A transition that isn't allowed gives 409 code `DELIVERY_STATUS`, `details: {"status": "<current>"}`. The message names the current status in words. The frontend treats a 409 whose `details.status` equals the requested target as success.
9. **Daily summary.**
   - `delivery_fees`: Σ `orders.delivery_fee` of orders created that day. It is already inside `sales_total` and is shown as a "of which delivery fees" line.
   - `fee_changes`: Σ `receivable` entries in `fee_adjustment` transactions occurring that day, signed.
   - `new_owed` keeps using `sale` and `payment` only, so the existing reconcile test holds.
10. **WhatsApp links are built on the server,** like receipt links (`orders::whatsapp::wa_link`), and returned on the delivery detail as `driver_whatsapp_url` and `customer_whatsapp_url`. This differs from spec §6 ("built on the phone") in where they are built, not in the text.
    - The driver link includes `{public_url}/driver/deliveries/{id}`.
    - Both are `null` when there is no driver.
11. **Driver-only users.** A session that has `deliveries.read_own` but not `deliveries.read` is "driver only".
    - Every route except `/driver/**`, `/login`, `/forbidden` and bare pages redirects to `/driver`.
    - After login the default target is `/driver`.
12. **Driver history** (`GET /me/deliveries`) is built from `delivery_assignments` of this driver whose delivery is no longer active for them (ended assignment, or the delivery is delivered/returned/cancelled). It returns the newest 20 deliveries: order number, locality, date, the amount collected-or-due when the job ended, and the delivery's current status.

## Review Focus

1. **A driver acting on a stale screen** after the job was reassigned, cancelled or already moved on. Expected: a reassigned or cancelled job is 404 with no detail leaked. A repeated tap on the step already done is 409 `DELIVERY_STATUS`, which the app treats as done. Tested in Task 7 (`reassigned_driver_gets_404`, `driver_cannot_see_other_drivers_job`) and Task 10 (`statusConflictIsDone`).
2. **The fee edited after part payment, then the order cancelled.** Expected: owed is always fee-correct, cancel reverses the *current* fee, `delivery_fee_income` nets to 0, and owed ends at 0. Tested in Task 5 (`fee_increase_after_full_payment_leaves_difference_owed`, `fee_cut_below_paid_is_rejected`) and Task 6 (`cancel_after_fee_edit_reverses_current_fee`).
3. **A deactivated driver with an open job.** Expected: their sessions end and they can't log in; the job shows `driver_active: false` and can be reassigned; they can't be assigned anything new. Tested in Task 3 (`deactivate_ends_sessions_and_blocks_login`) and Task 5 (`assign_inactive_driver_is_rejected`, `list_flags_inactive_driver`).
4. **Phone and address input as people type it**: `+91 98620-00000`, spaces, an empty landmark. Expected: phones normalised and an invalid one a field error on that input; blank optional text stored as NULL. Tested in Task 2 (`delivery_phone_is_normalised`, `delivery_requires_customer_and_address`).
5. **Check-in with a partial or duplicate line list, and a retry after a lost response.** Expected: 422 on `lines` and nothing written; the same key returns the same result once. Tested in Task 6 (`check_in_must_list_every_line_once`, `check_in_retry_with_same_key_once`).

---

## File structure

**Backend** (`backend/`):
- Create:
  - `migrations/20261110000000_deliveries.sql`
  - `src/deliveries/mod.rs` (router)
  - `src/deliveries/domain.rs` (status, reasons, pure transition rules)
  - `src/deliveries/input.rs` (validating the delivery block and the edit body)
  - `src/deliveries/read.rs` (owner list/detail, driver list/detail)
  - `src/deliveries/actions.rs` (assign, edit, status, retry)
  - `src/deliveries/close.rs` (cancel wrapper, check-in)
  - `src/deliveries/messages.rs` (WhatsApp texts)
  - `src/drivers/mod.rs`
- Modify:
  - `src/lib.rs` / `src/http/mod.rs` (mount routers)
  - `src/ledger/mod.rs`
  - `src/inventory/domain.rs`
  - `src/orders/checkout.rs`
  - `src/orders/cancel.rs`
  - `src/orders/returns.rs`
  - `src/orders/read.rs`
  - `src/orders/daily.rs`
  - `src/inventory/check.rs`
  - `src/customers/mod.rs` (last-address route)
  - `src/error.rs`
- Tests:
  - `tests/deliveries_schema.rs`, `tests/delivery_checkout.rs`, `tests/drivers.rs`, `tests/deliveries.rs`, `tests/delivery_close.rs`, `tests/driver_app.rs`
  - Extend `tests/orders_read.rs`, `tests/daily_summary.rs`, `tests/stock_check.rs`
  - Shared helpers: `tests/common/shop.rs` (new; see Task 2)

**Frontend** (`frontend/src/`):
- Create:
  - `modules/pos/delivery.ts` (+ test), `modules/pos/DeliveryFields.vue`
  - `modules/deliveries/`: `api.ts`, `labels.ts` (+ test), `DeliveriesPage.vue`, `DeliveryDetailPage.vue`, `AssignDialog.vue`, `EditDeliveryDialog.vue`, `StatusDialog.vue`, `CheckInDialog.vue`
  - `modules/driver/`: `DriverLayout.vue`, `DriverJobsPage.vue`, `DriverJobPage.vue`, `actions.ts` (+ test)
  - `modules/admin/DriverDialog.vue`, `modules/admin/drivers-api.ts`
- Modify:
  - `app/router.ts`, `app/guards.ts` (+ test), `app/nav.ts` (+ test), `app/layouts/NavIcon.vue`
  - `modules/pos/api.ts`, `modules/pos/cart.store.ts`, `modules/pos/ChargeSheet.vue`, `modules/pos/SaleDone.vue`
  - `modules/sales/SaleDetailPage.vue`, `modules/receipt/PublicReceiptPage.vue`, `modules/overview/DailySummaryCard.vue`
  - `modules/admin/UsersPage.vue`, `modules/auth/LoginPage.vue`

---

### Task 1: Migration

**Files:**
- Create: `backend/migrations/20261110000000_deliveries.sql`
- Test: `backend/tests/deliveries_schema.rs`

**Interfaces:**
- Produces:
  - Exactly the schema in spec §2.1–2.3.
  - `ledger_entries.account` CHECK plus `delivery_fee_income`; `ledger_transactions.kind` CHECK plus `fee_adjustment`.
  - `stock_movements.source_type` CHECK plus `delivery`.
  - The `ledger_immutable()` trigger on `delivery_events`.
  - Look up the current CHECK names in `20261108000000_sales.sql` and `20261109000000_returns.sql`.

- [ ] **Step 1: Write failing tests.**
  - `delivery_order_total_includes_fee`: an `orders` row with subtotal 100, fee 50, total 150 inserts; total 100 with fee 50 fails.
  - `counter_order_cannot_have_fee`: `fulfilment 'counter'` with fee 10 fails.
  - `retain_requires_driver_collector`: a `deliveries` row with `collector 'shop'` and `pay_method 'retain_from_collection'` fails.
  - `assigned_needs_driver_and_pay`: status `assigned` with NULL `current_driver_id` fails; with a driver but NULL `driver_pay` fails.
  - `delivered_needs_received_by`.
  - `one_open_assignment_per_delivery`: a second open `delivery_assignments` row fails.
  - `failed_other_needs_note`.
  - `delivery_events_cannot_be_changed`: UPDATE fails with `cannot be changed`.
  - `fee_income_account_and_fee_adjustment_kind_allowed`.
- [ ] **Step 2: Run** `cargo test --test deliveries_schema`. Expected: FAIL (relation "deliveries" does not exist).
- [ ] **Step 3: Write the migration.**
- [ ] **Step 4: Run** `cargo test --test deliveries_schema --test sales_schema --test returns_schema --test inventory_schema`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: schema"`

### Task 2: Checkout with delivery

**Files:**
- Create: `backend/src/deliveries/{mod.rs,domain.rs,input.rs}`, `backend/tests/common/shop.rs`
- Modify:
  - `backend/src/ledger/mod.rs` (`Account::DeliveryFeeIncome`, `TxnKind::FeeAdjustment`)
  - `backend/src/inventory/domain.rs` (`SourceType::Delivery`)
  - `backend/src/orders/checkout.rs`
  - `backend/src/customers/mod.rs`
  - `backend/src/lib.rs`
- Test: `backend/tests/delivery_checkout.rs`

**Interfaces:**
- Produces:
  - `deliveries::domain`:
    - `enum Status { ToSend, Assigned, PickedUp, Delivered, Failed, Returned, Cancelled }` with `as_str`/`parse`
    - `enum Collector { Shop, Driver }`
    - `enum PayMethod { RetainFromCollection, PaidByShop }`
    - `enum FailReason { … }` (five codes)
  - `deliveries::input`:
    - `struct DeliveryIn { recipient_name, recipient_phone, address_line, locality, landmark: Option, instructions: Option, fee: Decimal, driver_pay: Option<Decimal>, collector: Collector, pay_method: PayMethod }`
    - `fn validate_delivery(raw: &DeliveryRequest) -> AppResult<DeliveryIn>`. Field errors are named `delivery.<field>`. The defaults are those of spec §3.1.
  - `deliveries::create_for_order(tx: &mut PgConnection, actor: Uuid, order_id: Uuid, d: &DeliveryIn) -> AppResult<Uuid>`: inserts `deliveries` (`to_send`) and the first `delivery_events` row (`from NULL → to_send`).
  - `CheckoutRequest.delivery: Option<DeliveryRequest>`. With a delivery:
    - A customer is required: 422 on `customer_id`, "Pick or add the customer for this delivery."
    - `fulfilment='delivery'`, `delivery_fee=fee`, total = priced total + fee.
    - The overpayment check uses the new total.
    - The sale transaction has `[(Receivable, total), (SalesRevenue, -goods), (DeliveryFeeIncome, -fee)]`, skipping zero amounts.
  - `GET /customers/{id}/last-address` → `{recipient_name, recipient_phone, address_line, locality, landmark, instructions}` from that customer's newest delivery, or 404.
  - `tests/common/shop.rs`: move `Shop`, `shop()`, `key()`, `d()`, `uuid()`, `post()`, `sell()`, `on_hand()`, `order_owed()`, `ledger_is_sound()` from `tests/cancel.rs` here, made `pub`. Add:
    - `delivery_sale(sh, fee: &str, paid: &str) -> Value`: 1 Plate to a new customer with a fixed address
    - `account_total(pool, account) -> Decimal`

    Copy rather than move: leave `tests/cancel.rs` and `tests/returns.rs` as they are.

- [ ] **Step 1: Write failing tests.**
  - `delivery_checkout_posts_fee_and_creates_to_send`:
    - 2 Plates (₹200) + fee 50, pay later
    - total `"250.00"`, owed `"250.00"`
    - `account_total(delivery_fee_income)` = −50, `sales_revenue` = −200
    - stock 8
    - one `deliveries` row in `to_send` with `collector 'driver'`, `pay_method 'retain_from_collection'`
    - one event
  - `zero_fee_delivery_has_no_fee_entry`.
  - `delivery_requires_customer_and_address`: no customer → 422 `customer_id`; blank `locality` → 422 `delivery.locality`; nothing written.
  - `delivery_phone_is_normalised`: `"+91 98620-00000"` is stored as `9862000000`; `"12345"` → 422 `delivery.recipient_phone`.
  - `retain_with_shop_collector_is_rejected` → 422 `delivery.pay_method`.
  - `delivery_overpayment_uses_total_with_fee`: paying 250 on 200+50 is OK; 251 → the existing overpayment error.
  - `checkout_retry_with_delivery_same_key_once`.
  - `last_address_returns_newest_delivery` and `last_address_404_without_delivery`.
- [ ] **Step 2: Run** `cargo test --test delivery_checkout`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test delivery_checkout --test checkout --test money_ledger`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: sell an order for delivery with a fee"`

### Task 3: Driver accounts

**Files:**
- Create: `backend/src/drivers/mod.rs`
- Modify: `backend/src/lib.rs` (mount), `backend/src/admin/repo.rs` (reuse `create_user`)
- Test: `backend/tests/drivers.rs`

**Interfaces:**
- Consumes: `admin::repo::create_user`, `admin::password::hash_password`, `session::delete_all_for_user`, `normalize_phone`.
- Produces:
  - `GET /drivers` → `[{id, display_name, username, phone, default_pay, note, is_active, open_jobs: i64}]`, active first, then by name.
  - `POST /drivers` `{display_name, username, password, phone, default_pay, note?}` → 201 with the row. It creates the user with role `delivery_driver` and a `driver_profiles` row. The password rules are those of `create_admin` (look them up in `admin/mod.rs`).
    - A duplicate phone → 422 `phone`, "Another driver already has this phone number."
    - A duplicate username → the existing 409.
  - `PATCH /drivers/{id}` with any of `display_name, phone, default_pay, note, is_active`. Setting `is_active=false` deletes their sessions.
  - `POST /drivers/{id}/password` `{password}` → 204, and deletes their sessions.
  - All of them: `drivers.manage`, an audit row, 404 if the user has no `driver_profiles` row.
  - `pub async fn active_driver(conn, id) -> AppResult<DriverRef { id, display_name, phone, default_pay }>`: 422 on `driver_id` ("Pick an active driver.") when the user is missing, inactive or not a driver. Used by Task 5.

- [ ] **Step 1: Write failing tests.**
  - `owner_adds_driver_who_can_log_in`: the driver's `/auth/me` permissions are exactly the four `*_own` permissions.
  - `driver_phone_unique`.
  - `deactivate_ends_sessions_and_blocks_login`: an existing driver session gets 401; login is refused.
  - `reactivate_allows_login`.
  - `reset_password_ends_sessions`.
  - `drivers_need_drivers_manage`: a cashier gets 403.
  - `patch_non_driver_user_is_404`.
- [ ] **Step 2: Run** `cargo test --test drivers`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test drivers --test auth --test users_repo`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "drivers: add, edit, deactivate and reset drivers"`

### Task 4: Transition rules

**Files:**
- Modify: `backend/src/deliveries/domain.rs`, `backend/src/error.rs`
- Test: `#[cfg(test)]` in `domain.rs`

**Interfaces:**
- Produces:
  - `enum Action { Assign, Reassign, PickUp, Deliver, Fail, Retry, CheckIn, Cancel }`.
  - `fn next_status(from: Status, action: Action) -> Result<Status, Status /*current, for the error*/>`: exactly the transitions in spec §1. `Assign` comes only from `ToSend`, `Reassign` only from `Assigned`, `Retry`/`CheckIn` only from `Failed`, `Cancel` from `ToSend`/`Assigned`.
  - `fn status_label(s: Status) -> &'static str`: the copy table.
  - `AppError::delivery_status(current: Status)` gives 409 `DELIVERY_STATUS`, with the message "This delivery is {label}. That step isn't possible now." and `details {status}`.
  - `AppError::delivery_out()` gives 409 `DELIVERY_OUT`, and `AppError::delivery_not_done()` gives 409 `DELIVERY_NOT_DONE`, with the Ruling 4/5 copy.

- [ ] **Step 1: Write failing tests.**
  - `allowed_transitions`: a table of every allowed (from, action, to).
  - `everything_else_is_rejected`: iterate all 7 × 8 pairs; anything not in the table returns `Err(from)`.
- [ ] **Step 2: Run** `cargo test --lib deliveries::domain`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: status transition rules"`

### Task 5: Owner endpoints — list, detail, assign, edit, status, retry

**Files:**
- Create: `backend/src/deliveries/{read.rs,actions.rs,messages.rs}`
- Modify: `backend/src/deliveries/mod.rs`
- Test: `backend/tests/deliveries.rs`

**Interfaces:**
- Consumes:
  - `domain::next_status`, `drivers::active_driver`, `input::validate_delivery`
  - `orders::whatsapp::{wa_link, money_short}`, `ledger::{post, order_owed}`, idempotency
- Produces:
  - `GET /deliveries?status=to_send|out|failed|done` (`out` = assigned + picked_up; `done` = delivered/returned/cancelled with `closed_at`/`delivered_at` in the last 7 days).
    - Items: `{id, order_id, order_number, status, recipient_name, locality, driver: {id, name, active}|null, amount_to_collect, fee, since /*time of last status change*/, fail_reason|null}`, oldest `since` first, except `done` which is newest first.
    - `counts: {to_send, out, failed}` on every response.
  - `GET /deliveries/{id}` → `DeliveryDetail`:
    - all delivery fields
    - `order: {id, number, total /*Ruling 1*/, paid, owed}`
    - `items: [{name, code, quantity}]`
    - `driver`, `events: [{from, to, kind, actor_name, note, changes, at}]`, `attempts: [{driver_name, reason, note, trip_pay, at}]`
    - `amount_to_collect`: owed when `collector = driver`, else "0.00"
    - `driver_whatsapp_url`, `customer_whatsapp_url` (Ruling 10)
    - `maps_url`: `https://www.google.com/maps/search/?api=1&query=` + url-encoded `address_line, locality`
  - `POST /deliveries/{id}/assign` `{driver_id, driver_pay?}`:
    - `ToSend` → Assign, `Assigned` → Reassign (same driver → 422 `driver_id`, "This driver already has it.")
    - Ends the open assignment (`reassigned`), inserts a new one, sets `current_driver_id`, `assigned_at`, `driver_pay` (Ruling 7), writes an event.
    - Returns `DeliveryDetail`.
  - `PATCH /deliveries/{id}` with any spec §3.2 fields, plus `Idempotency-Key`:
    - Only `ToSend`/`Assigned` (else `delivery_status`).
    - Locks per the Global Constraints. Ruling 6 for the fee.
    - One `edit` event whose `changes` holds only the changed fields.
    - Returns `DeliveryDetail`.
  - `POST /deliveries/{id}/status` `{to: "picked_up"|"delivered"|"failed", received_by?, reason?, note?}`: shared with Task 7 through `actions::change_status(tx, actor, delivery_id, StatusChange, by_driver: Option<Uuid>)`.
    - `received_by` is 1–80 characters (422 `received_by`, "Write who received it.").
    - `reason` is required for failed (422 `reason`); `note` is required for `other` (422 `note`).
    - Delivered/failed end the assignment (`completed`/`failed`).
    - Failed inserts `delivery_attempts` with the current driver.
    - The event note for an owner acting for the driver is "Marked by {owner name} for {driver name}".
  - `POST /deliveries/{id}/retry` `{driver_id, driver_pay?, trip_pay?}`: `Failed` → `Assigned`. It sets the latest attempt's `trip_pay` (default 0) and assigns as above.

- [ ] **Step 1: Write failing tests** (use `tests/common/shop.rs` and a driver made through `POST /drivers`):
  - `assign_shows_job_and_prefills_pay_from_driver_default`: default 40, no pay on the delivery → `driver_pay "40.00"`.
  - `assign_keeps_pay_typed_at_checkout`.
  - `reassign_ends_previous_assignment`: two assignment rows, the first ended `reassigned`.
  - `assign_inactive_driver_is_rejected` → 422 `driver_id`.
  - `list_flags_inactive_driver`: assign, then deactivate → the `out` item has `driver.active == false`.
  - `list_tabs_and_counts`.
  - `status_out_of_order_is_409`: deliver from `assigned` → 409 `DELIVERY_STATUS` with `details.status == "assigned"`.
  - `owner_marks_delivered_for_driver`: the event actor is the owner and the note names the driver.
  - `failed_needs_reason_and_other_needs_note`.
  - `retry_sets_trip_pay_and_reassigns`: fail, retry with another driver and `trip_pay` 20 → the attempt `trip_pay "20.00"`, status `assigned`.
  - `fee_increase_after_full_payment_leaves_difference_owed`: fee 50, paid 250, edit fee to 80 → owed `"30.00"`, one `fee_adjustment` transaction, the original sale transaction unchanged.
  - `fee_cut_below_paid_is_rejected` → 422 `fee`, nothing written.
  - `edit_after_pickup_is_409`.
  - `edit_records_only_changed_fields`.
  - `detail_whatsapp_and_maps_links`: the driver text contains the order number, `Collect: ₹250` and `/driver/deliveries/{id}`.
  - `concurrent_assign_leaves_one_open_assignment`: `tokio::join!` two assigns to different drivers → exactly one open assignment, and it matches `current_driver_id`.
- [ ] **Step 2: Run** `cargo test --test deliveries`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test deliveries`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: dispatch, edit, status and retry for the owner"`

### Task 6: Cancel and check-in

**Files:**
- Create: `backend/src/deliveries/close.rs`
- Modify: `backend/src/orders/cancel.rs` (Ruling 3, 4), `backend/src/orders/returns.rs` (Ruling 5)
- Test: `backend/tests/delivery_close.rs`

**Interfaces:**
- Produces:
  - `orders::cancel::{lock_order, cancel_core, LockedOrder { id, customer_id, status, goods_total, fee, current_total }}`.
    - `cancel()` keeps its signature and behaviour for counter sales.
    - For an order with a delivery it locks the delivery row and applies Ruling 4.
  - `POST /deliveries/{id}/cancel` `{reason?}` (Ruling 4) → `DeliveryDetail`.
  - `POST /deliveries/{id}/check-in` `{lines: [{order_line_id, disposition}], trip_pay?}` with `Idempotency-Key`:
    - `Failed` only.
    - Every order line must be listed exactly once (422 `lines`, "Choose sellable or damaged for every item.").
    - It runs `cancel_core` with reason "Delivery returned".
    - Then a `damage` movement (−quantity, `SourceType::Delivery`, the delivery id) per damaged line.
    - The latest attempt's `trip_pay` is set, the delivery becomes `returned` with `closed_at`, and an event is written.
    - Returns `DeliveryDetail`.

- [ ] **Step 1: Write failing tests.**
  - `cancel_before_pickup_restores_stock_and_reverses_fee`: fee 50, paid 250 cash → refund cash 250, stock back, `delivery_fee_income` total 0, delivery `cancelled`, assignment ended `cancelled`.
  - `cancel_after_fee_edit_reverses_current_fee`: fee 50 → 80, cancel → `delivery_fee_income` 0, owed 0, `ledger_is_sound`.
  - `order_cancel_route_on_picked_up_delivery_is_409`: `/orders/{id}/cancel` → `DELIVERY_OUT`.
  - `counter_cancel_unchanged`: the existing `tests/cancel.rs` still passes (run it in Step 4).
  - `check_in_restocks_only_sellable`: 2 lines, one sellable and one damaged → the sellable product is back to 10, the damaged one stays at 9 (sale_cancel +1, damage −1), order `cancelled`, delivery `returned`.
  - `check_in_must_list_every_line_once`: a missing line → 422; a duplicate line → 422; nothing written.
  - `check_in_retry_with_same_key_once`.
  - `check_in_only_from_failed` → 409 `DELIVERY_STATUS`.
  - `return_before_delivered_is_409` → `DELIVERY_NOT_DONE`; after delivered, the return works and doesn't touch `delivery_fee_income`.
- [ ] **Step 2: Run** `cargo test --test delivery_close`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test delivery_close --test cancel --test returns --test inventory_ledger`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: cancel and check in returned goods"`

### Task 7: Driver endpoints

**Files:**
- Modify: `backend/src/deliveries/{read.rs,mod.rs}`
- Test: `backend/tests/driver_app.rs`

**Interfaces:**
- Consumes: `actions::change_status` with `by_driver: Some(user.id)`.
- Produces:
  - `GET /me/deliveries` → `{active: [{id, order_number, status, locality, recipient_name, amount_to_collect}], history: [{id, order_number, locality, at, amount, status}]}` (Ruling 12).
  - `GET /me/deliveries/{id}` → while active for this driver: `{id, order_number, status, recipient_name, recipient_phone, address_line, locality, landmark, instructions, items: [{name, code, quantity}], amount_to_collect, collector, pay_method, driver_pay, maps_url}`. Otherwise, if this driver ever had it: the history shape. Otherwise 404. No prices or costs.
  - `POST /me/deliveries/{id}/status`: same body as the owner's status endpoint. The driver must be `current_driver_id`, else 404.
  - All take `deliveries.read_own` / `deliveries.update_own`, and the driver from the session only.

- [ ] **Step 1: Write failing tests.**
  - `driver_sees_only_own_active_jobs`: two drivers, one job each.
  - `driver_cannot_see_other_drivers_job` → 404 on GET and POST.
  - `reassigned_driver_gets_404`: driver A had it, then it was reassigned to B → A's POST gets 404, and A's GET returns the history shape with no `recipient_phone` key.
  - `delivered_job_moves_to_history_without_address`.
  - `driver_full_flow`: picked_up → delivered with `received_by` → event actor = driver.
  - `driver_double_tap_is_409_with_status`.
  - `driver_detail_has_no_prices`: no key containing `price` or `cost` anywhere in the JSON.
  - `staff_endpoints_forbidden_for_driver`: `/api/v1/orders`, `/api/v1/deliveries`, `/api/v1/customers` → 403.
- [ ] **Step 2: Run** `cargo test --test driver_app`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --test driver_app --test deliveries`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "deliveries: driver's own jobs and status updates"`

### Task 8: Read models, receipt, daily summary, nightly check

**Files:**
- Modify: `backend/src/orders/read.rs` (Rulings 1, 2), `backend/src/orders/whatsapp.rs` (fee line), `backend/src/orders/daily.rs` (Ruling 9), `backend/src/inventory/check.rs` (Ruling 1 bound)
- Test: extend `tests/orders_read.rs`, `tests/daily_summary.rs`, `tests/stock_check.rs`

**Interfaces:**
- Produces:
  - `OrderDetail` gains `fulfilment`, `delivery_fee` (current), and `delivery: Option<{id, status, driver_name}>`.
  - `OrderSummary` gains `fulfilment` and `delivery_status: Option<String>`.
  - `PublicReceipt` gains `delivery_fee` (`"0.00"` for counter sales). The receipt text adds a `Delivery: ₹50` line before Total when the fee is > 0.
  - The daily summary gains `delivery_fees` and `fee_changes`.
  - The nightly check's confirmed-order bound uses the current total.

- [ ] **Step 1: Write failing tests.**
  - `detail_shows_current_fee_and_total_after_edit`: fee 50 → 80 → `total "280.00"`, `delivery_fee "80.00"`.
  - `fully_returned_delivery_order_is_returned_status`: delivered, return all goods, the fee was paid → `payment_status "returned"`.
  - `public_receipt_has_delivery_fee`; `receipt_text_has_delivery_line` (unit test in `whatsapp.rs`).
  - `daily_summary_delivery_fees_and_fee_changes`; the existing `daily_summary_reconciles` still passes.
  - `stock_check_accepts_fee_increase`: after a fee increase, no `receivable_out_of_range`.
- [ ] **Step 2: Run** `cargo test --test orders_read --test daily_summary --test stock_check`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test && cargo clippy --all-targets -- -D warnings && cargo sqlx prepare --check -- --all-targets`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "orders: delivery fee on sales, receipts, summary and checks"`

### Task 9: Frontend — routing, nav, API types

**Files:**
- Create: `frontend/src/modules/deliveries/{api.ts,labels.ts,labels.test.ts}`
- Modify:
  - `frontend/src/app/guards.ts` (+ `guards.test.ts`), `frontend/src/app/router.ts`
  - `frontend/src/app/nav.ts` (+ `nav.test.ts`), `frontend/src/app/layouts/NavIcon.vue`
  - `frontend/src/modules/auth/LoginPage.vue`, `frontend/src/modules/auth/session.store.ts`
  - `frontend/src/modules/pos/api.ts`

**Interfaces:**
- Produces:
  - `guards.ts`:
    - `GuardSession` gains `isDriverOnly: boolean`.
    - `authGuard` sends a driver-only session from any non-public, non-bare route outside `/driver` to `{ path: '/driver' }`.
    - `homeFor(session): '/' | '/driver'`, which LoginPage uses when `next` is absent or not allowed.
  - `session.store`: `isDriverOnly` getter (Ruling 11).
  - `nav.ts`: `{ label: 'Deliveries', to: '/deliveries', perm: 'deliveries.read', icon: 'deliveries' }` after Sales.
  - Routes:
    - `/deliveries`, `/deliveries/:id` (perm `deliveries.read`, AppLayout)
    - `/driver`, `/driver/deliveries/:id` (perm `deliveries.read_own`, `DriverLayout`)
  - `deliveries/api.ts`: types matching Tasks 5–7, plus `listDeliveries(tab)`, `getDelivery(id)`, `assign`, `editDelivery(id, body, key)`, `setStatus(id, body)`, `retry`, `cancelDelivery(id, body, key)`, `checkIn(id, body, key)`, `myJobs()`, `myJob(id)`, `setMyStatus(id, body)`.
  - `labels.ts`: `statusLabel(code)`, `reasonLabel(code)`, `reasons` (ordered list), copied from the Global Constraints tables.
  - `pos/api.ts`: `CheckoutBody.delivery?`, plus the `OrderDetail`/`OrderSummary`/`DailySummary`/`PublicReceipt` fields from Task 8.

- [ ] **Step 1: Write failing tests.**
  - `guards.test.ts`: a driver-only session at `/` → `/driver`; at `/sales` → `/driver`; at `/driver/deliveries/x` → allowed; at `/r/abc` (bare) → allowed; an admin at `/driver` → allowed; `homeFor`.
  - `nav.test.ts`: Deliveries is visible with `deliveries.read`.
  - `labels.test.ts`: every status and reason code has a label; an unknown code falls back to the code itself.
- [ ] **Step 2: Run** `pnpm -C frontend test -- guards nav labels`. Expected: FAIL.
- [ ] **Step 3: Implement.** Use placeholder page components only for the new routes; Tasks 11–12 fill them.
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "frontend: delivery routes, driver-only routing, API types"`

### Task 10: Frontend — sell for delivery

**Files:**
- Create: `frontend/src/modules/pos/delivery.ts` (+ `delivery.test.ts`), `frontend/src/modules/pos/DeliveryFields.vue`
- Modify: `frontend/src/modules/pos/cart.store.ts` (+ test), `ChargeSheet.vue`, `SaleDone.vue`

**Interfaces:**
- Produces in `delivery.ts`:
  - `interface DeliveryDraft { recipientName, recipientPhone, addressLine, locality, landmark, instructions, fee: string, driverPay: string, collector: 'driver'|'shop', payMethod: 'retain_from_collection'|'paid_by_shop' }`
  - `emptyDraft()`
  - `draftFromLastAddress(addr, customer)`
  - `deliveryBody(d): CheckoutBody['delivery']`: trims, sends blanks as null, omits a blank `driver_pay`, and forces `paid_by_shop` when the collector is shop
  - `draftErrors(d): Record<string,string>` (client-side required fields, matching the server field names `delivery.*`)
  - `totalWithFee(goods: string, fee: string): string` (paise maths)
- Produces in `cart.store`:
  - `fulfilment: 'counter'|'delivery'`, `delivery: DeliveryDraft`
  - `totals.total` includes the fee in delivery mode
  - both are persisted with the cart and cleared after a sale
  - the pending checkout key covers them (it already hashes the body)
- Behaviour:
  - **ChargeSheet** gets a Counter / Delivery segmented switch.
  - Delivery mode shows `DeliveryFields`. When a saved customer is picked it calls `GET /customers/{id}/last-address` and prefills the empty fields only.
  - The Cash field defaults to 0 in delivery mode, so the default is "driver collects".
  - The summary reads "Goods ₹200 + Delivery ₹50 = ₹250" and "Driver collects ₹250" or "Paid".
  - Server `delivery.*` field errors show on the matching input.
  - **SaleDone** shows "Assign driver" (to `/deliveries/{id}`) for a delivery.

- [ ] **Step 1: Write failing tests** in `delivery.test.ts`:
  - `deliveryBody` trims and nulls blanks, and forces `paid_by_shop` for the shop collector
  - `draftErrors` requires name, phone, address and locality
  - `totalWithFee('200.00','50')` = `'250.00'`
  - `draftFromLastAddress` doesn't overwrite typed fields

  In `cart.store.test.ts`: delivery mode total includes the fee; switching back to counter drops the fee.
- [ ] **Step 2: Run** `pnpm -C frontend test -- delivery cart.store`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px against the local API (`pnpm build` + `vite preview`): sell a delivery with a fee, and the total and done screen. Expected: PASS, with the behaviour as described.
- [ ] **Step 5: Commit** `git commit -m "pos: sell an order for delivery"`

### Task 11: Frontend — Deliveries screens

**Files:**
- Create: `frontend/src/modules/deliveries/{DeliveriesPage.vue,DeliveryDetailPage.vue,AssignDialog.vue,EditDeliveryDialog.vue,StatusDialog.vue,CheckInDialog.vue}`
- Modify: `frontend/src/modules/deliveries/labels.ts` (+ test), `frontend/src/modules/sales/SaleDetailPage.vue`, `frontend/src/modules/sales/SalesPage.vue`

**Interfaces:**
- Consumes:
  - Task 9 API
  - `attemptSession`/`persistedKeys` from `shared/idempotency.ts` with storage key `jj.delivery.v1` and scopes `edit:{id}`, `check-in:{id}`
  - the existing `jj.undo.v1` `cancel:{order_id}` scope for cancel, so a cancel started from the sale page and one started from the delivery page share a key
  - `openWhatsApp`
- Produces in `labels.ts`: `actionsFor(status, hasDriver): Array<'assign'|'reassign'|'edit'|'cancel'|'picked_up'|'delivered'|'failed'|'retry'|'check_in'>`, matching spec §1.
- Behaviour:
  - **DeliveriesPage:** tabs with counts. Cards as in spec §5; the `since` age reads "2 h", "3 d". An inactive driver shows a red "inactive" badge. The tab is kept in the query string.
  - **DeliveryDetailPage:**
    - status, history and attempts
    - `tel:` and Maps links, items, money
    - WhatsApp buttons (disabled without a driver)
    - action buttons from `actionsFor`
    - each dialog reloads the detail on success
    - 409 `DELIVERY_STATUS` shows its message and reloads
  - **AssignDialog:** active drivers (`GET /drivers`), pay prefilled per Ruling 7; also used for Retry, with a "Pay for the failed trip" field (default 0).
  - **CheckInDialog:** a Sellable / Damaged toggle per item, default Sellable; failed-trip pay.
  - **SaleDetailPage:**
    - a delivery panel linking to the delivery
    - Cancel sale is hidden when the delivery is `picked_up`/`delivered`/`failed`/`returned`, with the Ruling 4 text in its place
    - Return items appears only when the delivery is `delivered`
  - **SalesPage:** a small "Delivery · {status}" badge.

- [ ] **Step 1: Write failing tests** in `labels.test.ts`: `actionsFor` per status (to_send: assign, edit, cancel; assigned: reassign, edit, cancel, picked_up; picked_up: delivered, failed; failed: retry, check_in; delivered/returned/cancelled: none).
- [ ] **Step 2: Run** `pnpm -C frontend test -- labels`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px: assign, reassign, edit the fee, mark failed on the driver's behalf, retry, check in, and cancel from the sale page. Expected: PASS, with the behaviour as described.
- [ ] **Step 5: Commit** `git commit -m "deliveries: dispatch screens for the owner"`

### Task 12: Frontend — driver area

**Files:**
- Create: `frontend/src/modules/driver/{DriverLayout.vue,DriverJobsPage.vue,DriverJobPage.vue,actions.ts,actions.test.ts}`

**Interfaces:**
- Produces in `actions.ts`:
  - `primaryAction(status): 'picked_up'|'delivered'|null`
  - `canFail(status): boolean` (only `picked_up`)
  - `statusConflictIsDone(err: ApiError, requested: string): boolean` (Ruling 8)
- Behaviour:
  - **DriverLayout:** a header with the shop name, the driver's name and Log out. There is no nav.
  - **DriverJobsPage:** active job cards (locality large, amount to collect, status) and History below. It refreshes on mount, on `visibilitychange` to visible, and with a Refresh button (pull-to-refresh is the browser's native one).
  - **DriverJobPage:**
    - big Call (`tel:`) and Map buttons
    - address, landmark, instructions, items
    - "Collect ₹X" or "Already paid"
    - the primary action button, then a secondary "Couldn't deliver"
    - Delivered asks "Who received it?"; Couldn't deliver shows the reason radio list and a note (required for Other)
    - 404 → "This job is no longer assigned to you." and back to the list
    - a 409 where `statusConflictIsDone` → treat as done and reload
  - Touch targets ≥ 48 px, one column, readable at 360 px.

- [ ] **Step 1: Write failing tests** in `actions.test.ts`:
  - `primaryAction` per status
  - `canFail`
  - `statusConflictIsDone` true for 409 `DELIVERY_STATUS` with `details.status === requested`, false for other codes or statuses
- [ ] **Step 2: Run** `pnpm -C frontend test -- actions`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px, logged in as a driver: land on `/driver`, can't reach `/sales`, picked up → delivered, a reassigned job disappears with the message. Expected: PASS, with the behaviour as described.
- [ ] **Step 5: Commit** `git commit -m "driver: my deliveries on the driver's phone"`

### Task 13: Frontend — drivers on the Users page

**Files:**
- Create: `frontend/src/modules/admin/{DriverDialog.vue,drivers-api.ts}`
- Modify: `frontend/src/modules/admin/UsersPage.vue`

**Interfaces:**
- Consumes: Task 3 endpoints.
- Behaviour:
  - A **Drivers** section above the users list: name, phone, default pay, open jobs, active badge.
  - **Add driver** opens DriverDialog (name, phone, username, password, default pay, note).
  - Row actions: Edit (the same dialog without username/password), Reset password (a small dialog), Deactivate/Reactivate. Deactivating a driver with open jobs confirms: "{name} has {n} open deliveries. They stay assigned until you reassign them."
  - Server field errors show on their inputs.
- [ ] **Step 1: Implement.** It is UI-only; the logic is covered by the Task 3 backend tests.
- [ ] **Step 2: Run** `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Then manually check at 360 px: add a driver, log in as them in a private window, deactivate them, and see the private window's next request fail. Expected: PASS, with the behaviour as described.
- [ ] **Step 3: Commit** `git commit -m "users: add and manage drivers"`

### Task 14: Receipts, summary, docs

**Files:**
- Modify:
  - `frontend/src/modules/receipt/PublicReceiptPage.vue` (a Delivery line when the fee is > 0)
  - `frontend/src/modules/overview/DailySummaryCard.vue` ("of which delivery fees", and "Delivery fee changes" when not 0)
  - `Delivery.md` (v2.1: a dated note at the top pointing to the 6a spec's "Changes from Delivery.md"; §2 `to_send`; §5 OTP marked later; §9.1 zones removed)
  - `Architecture.md` (§10.1 `fulfilment` delivery built; §10.3 accounts built: `delivery_fee_income`, the `fee_adjustment` kind, cancel entries per Ruling 3; §15 endpoints; §20 status line; §20 open question "Delivery zones and fees" answered: typed per order, 7 Nov 2026)
  - the 6a spec (a dated note on Ruling 10)

- [ ] **Step 1: Implement and update the docs.**
- [ ] **Step 2: Run** the full checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo sqlx prepare --check -- --all-targets`, `pnpm -C frontend test && pnpm -C frontend typecheck && pnpm -C frontend build`. Expected: all pass.
- [ ] **Step 3: Commit** `git commit -m "docs: Phase 6a deliveries built"`
