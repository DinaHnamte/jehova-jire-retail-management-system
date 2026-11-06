# Phase 6a: Deliveries (design)

**Date:** 7 Nov 2026
**Implements:** Architecture.md §20 Phase 6 (first half); [Delivery.md](../../../Delivery.md) phase 1, adapted below
**Status:** approved in chat on 7 Nov 2026; awaiting review of this written version

## Goal

Sell an order for delivery, hand it to one of a few trusted drivers,
and know at any moment where each delivery stands: not sent yet, out
with which driver, failed and waiting for a decision, or done. Drivers
log in on their own phones and see only their own jobs.

Driver money (custody, pay, handovers, payouts, settlement) is
**Phase 6b**, with its own spec section and plan. 6a deploys and is
usable on its own.

## Decisions made with the owner (7 Nov 2026)

| Question | Answer |
|---|---|
| Who delivers? | **One or two trusted helpers**, paid per trip, who hold the customer's cash until they hand it over. They use their own phones and need logins. The owner does all manager and cashier work. |
| Fee and driver pay | **Typed in on each order.** No zones, no defaults table. Either can be ₹0. |
| Who collects / how is the driver paid? | **Every combination happens.** Default: driver collects, driver keeps their pay. Editable on each order before pickup. |
| Proof of delivery | **Driver types who received it.** No OTP, no photos. |
| Pay for a failed trip | **Decided each time** (typed in, default ₹0). |
| Split | **6a Deliver, then 6b Driver money**, like Phase 5. |

## Changes from Delivery.md (v2.0)

- **No delivery zones** and no fee-quote endpoint: fee and driver pay are typed in.
- **No OTP proof:** `delivery_proof` is always the recipient name.
- **One back-office user:** the owner (admin) does dispatching, check-in and later handovers on one screen. No separate cashier flow.
- **The owner may act on a driver's behalf** (picked up, delivered, failed), for when a driver's phone is dead. The event records the owner as actor.
- **Arrangement is columns on `deliveries`**, not a versioned `delivery_arrangements` table. Edits before pickup are written to `delivery_events` and the audit log.
- **Status `to_send`** replaces `awaiting_dispatch` (same meaning, plainer word on screen and in code).
- **Driver money moves to 6b.** In 6a, `driver_pay` and `pay_method` are stored and shown but not posted. A driver's handed-over cash is recorded by the owner as an ordinary cash payment on the order (5a payment flow). 6b posts driver money from its deploy date onward; trips before that are settled by hand.

Delivery.md and Architecture.md §10 get updated to match when this spec is approved for implementation.

## Scope

**In 6a**

- Delivery orders from the Sell screen (customer required, recipient and address snapshot, fee, driver pay, collector, driver pay method)
- Delivery fee in the order total, the sale ledger posting, the receipt page and the WhatsApp receipt
- Deliveries screen for the owner: lists, detail, assign/reassign, edit before pickup, cancel, status on a driver's behalf, retry, check-in
- Driver area `/driver`: my jobs, detail, picked up / delivered / failed
- Driver accounts from the Users page: add, reset password, deactivate/reactivate, default trip pay
- WhatsApp and `tel:` links for the driver and the customer
- Prefilling the address from the customer's last delivery

**Not in 6a**

- Driver collections recorded by the driver, custody, payable, keep-own-pay, handovers, payouts, settlement screens, cash position report (6b)
- Zones, OTP, photos, push notifications, offer board
- Creating staff users other than drivers

## 1. Lifecycle

```
to_send ──assign──▶ assigned ──picked up──▶ picked_up ──▶ delivered
   │                  │  ▲                       │
   │                  │  └──── retry ───── failed ◀┘
   └──cancel──┐  cancel│                      │
              ▼        ▼                       ▼ check in
           cancelled ◀─────────────────── returned (order cancelled)
```

| Transition | Who | Permission | Requires | Side effects |
|---|---|---|---|---|
| → `to_send` | system | — | — | created by checkout with a delivery block |
| `to_send` → `assigned` | owner | `deliveries.assign` | active driver; driver pay (prefilled from the driver's default when blank) | new `delivery_assignments` row |
| `assigned` → `assigned` | owner | `deliveries.assign` | different active driver | previous assignment ended (`reassigned`); old driver loses access |
| `assigned` → `picked_up` | assigned driver, or owner | `deliveries.update_own` / `deliveries.assign` | — | fee, pay, collector, pay method and address frozen |
| `picked_up` → `delivered` | assigned driver, or owner | same | `received_by` (1–80 chars) | `delivered_at` set; assignment ended (`completed`) |
| `picked_up` → `failed` | assigned driver, or owner | same | reason; note when reason is `other` | `delivery_attempts` row; assignment ended (`failed`) |
| `failed` → `assigned` | owner | `deliveries.assign` | active driver (same or other); optional pay for the failed trip | attempt's `trip_pay` set; new assignment |
| `failed` → `returned` | owner | `deliveries.check_in` | disposition per order line; optional pay for the failed trip | order cancelled (§3.3); damaged lines written off |
| `to_send`/`assigned` → `cancelled` | owner | `deliveries.cancel` | optional reason | order cancelled (§3.3); assignment ended (`cancelled`) |

Failed reasons: `customer_unavailable`, `address_not_found`,
`customer_refused`, `unreachable_by_phone`, `other`.

Every transition:

- locks the `deliveries` row (`SELECT … FOR UPDATE`), checks the current status, and returns **409** with a message naming the current status when it does not match ("This delivery is already picked up; it can only be delivered or marked failed");
- for driver calls, checks `current_driver_id = session.user_id` and returns **404** otherwise (no hint that the job exists);
- writes one `delivery_events` row (from, to, actor, time, note) and an audit row.

Status is the only state the backend trusts. Repeating a status call
that has already happened (driver double-tap) returns 409; the
frontend treats a 409 whose current status equals the requested one as
success and refreshes.

## 2. Data model

Migration `20261110000000_deliveries.sql`.

### 2.1 Orders

- `orders.fulfilment` CHECK widened to `('counter', 'delivery')`.
- New column `orders.delivery_fee NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (delivery_fee >= 0)`.
- `orders_total_matches` becomes `total = subtotal - order_discount + delivery_fee AND total >= 0`.
- `CHECK (fulfilment = 'delivery' OR delivery_fee = 0)`.

`orders.delivery_fee` is the fee **as charged at checkout**; later fee
edits (§3.2) post adjustments and update `deliveries.fee`, so the
current fee lives on `deliveries.fee`, and what the customer owes is
always the ledger receivable. Every screen and the receipt show the
fee from `deliveries.fee` and the total as `orders.total` plus the sum
of fee adjustments. Returns (5b) refund goods only, never the delivery
fee.

### 2.2 Ledger

- `ledger_entries.account` CHECK adds `delivery_fee_income`.
- `ledger_transactions.kind` CHECK adds `fee_adjustment`.

### 2.3 New tables

```sql
CREATE TABLE driver_profiles (
    user_id     UUID PRIMARY KEY REFERENCES users (id) ON DELETE RESTRICT,
    phone       TEXT NOT NULL UNIQUE CHECK (phone ~ '^[6-9][0-9]{9}$'),
    default_pay NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (default_pay >= 0),
    note        TEXT CHECK (length(note) <= 500),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Active/inactive is users.is_active; there is no second flag.

CREATE TABLE deliveries (
    id                UUID PRIMARY KEY,
    order_id          UUID NOT NULL UNIQUE REFERENCES orders (id) ON DELETE RESTRICT,
    status            TEXT NOT NULL CHECK (status IN
                      ('to_send', 'assigned', 'picked_up', 'delivered', 'failed', 'returned', 'cancelled')),
    recipient_name    TEXT NOT NULL CHECK (length(recipient_name) BETWEEN 1 AND 120),
    recipient_phone   TEXT NOT NULL CHECK (recipient_phone ~ '^[6-9][0-9]{9}$'),
    address_line      TEXT NOT NULL CHECK (length(address_line) BETWEEN 1 AND 300),
    locality          TEXT NOT NULL CHECK (length(locality) BETWEEN 1 AND 80),
    landmark          TEXT CHECK (length(landmark) <= 120),
    instructions      TEXT CHECK (length(instructions) <= 300),
    fee               NUMERIC(12,2) NOT NULL CHECK (fee >= 0),
    driver_pay        NUMERIC(12,2) CHECK (driver_pay >= 0),   -- NULL until assignment at the latest
    collector         TEXT NOT NULL CHECK (collector IN ('shop', 'driver')),
    pay_method        TEXT NOT NULL CHECK (pay_method IN ('retain_from_collection', 'paid_by_shop')),
    current_driver_id UUID REFERENCES users (id) ON DELETE RESTRICT,
    received_by       TEXT CHECK (length(received_by) BETWEEN 1 AND 80),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    assigned_at       TIMESTAMPTZ,
    picked_up_at      TIMESTAMPTZ,
    delivered_at      TIMESTAMPTZ,
    closed_at         TIMESTAMPTZ,   -- returned or cancelled
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (pay_method <> 'retain_from_collection' OR collector = 'driver'),
    CHECK ((status IN ('assigned', 'picked_up')) = (current_driver_id IS NOT NULL)),
    CHECK (status NOT IN ('assigned', 'picked_up', 'delivered') OR driver_pay IS NOT NULL),
    CHECK ((status = 'delivered') = (received_by IS NOT NULL))
);
CREATE INDEX deliveries_status_idx ON deliveries (status, created_at DESC);
CREATE INDEX deliveries_driver_idx ON deliveries (current_driver_id) WHERE current_driver_id IS NOT NULL;

CREATE TABLE delivery_assignments (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    driver_id   UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    assigned_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at    TIMESTAMPTZ,
    end_reason  TEXT CHECK (end_reason IN ('reassigned', 'completed', 'failed', 'cancelled')),
    CHECK ((ended_at IS NULL) = (end_reason IS NULL))
);
CREATE UNIQUE INDEX delivery_assignments_one_open ON delivery_assignments (delivery_id) WHERE ended_at IS NULL;
CREATE INDEX delivery_assignments_driver_idx ON delivery_assignments (driver_id, assigned_at DESC);

CREATE TABLE delivery_attempts (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    driver_id   UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    reason      TEXT NOT NULL CHECK (reason IN ('customer_unavailable', 'address_not_found',
                'customer_refused', 'unreachable_by_phone', 'other')),
    note        TEXT CHECK (length(note) <= 500),
    trip_pay    NUMERIC(12,2) CHECK (trip_pay >= 0),   -- set when the owner retries or checks in
    recorded_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (reason <> 'other' OR note IS NOT NULL)
);

CREATE TABLE delivery_events (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    from_status TEXT,
    to_status   TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('status', 'edit')),
    actor_id    UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    note        TEXT CHECK (length(note) <= 500),
    changes     JSONB,   -- for 'edit': {field: [old, new]}
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX delivery_events_delivery_idx ON delivery_events (delivery_id, occurred_at);
-- delivery_events is immutable (ledger_immutable trigger).
```

`delivery_attempts.trip_pay` is the one column updated after insert
(NULL → amount, once, when the owner retries or checks in). No trigger
makes attempts immutable.

## 3. Behaviour

### 3.1 Checkout with delivery

`POST /orders/checkout` takes an optional `delivery` block:

```json
"delivery": {
  "recipient_name": "Lalthangi", "recipient_phone": "9862000000",
  "address_line": "House 12, Chanmari West", "locality": "Chanmari",
  "landmark": "near the church", "instructions": "call before coming",
  "fee": "50", "driver_pay": "40",
  "collector": "driver", "pay_method": "retain_from_collection"
}
```

- A delivery needs a customer (`customer_id` or `new_customer`).
- `driver_pay` is optional; `collector` defaults to `driver`, `pay_method` to `retain_from_collection` (or `paid_by_shop` when `collector = shop`).
- Same transaction as today, plus: `orders.fulfilment = 'delivery'`, `delivery_fee = fee`, `total` includes the fee; the `sale` transaction gets an extra credit to `delivery_fee_income` for the fee (no entry when the fee is 0); a `deliveries` row in `to_send` and its first `delivery_events` row.
- Payments in the checkout request work as now (the owner may take payment up front).
- The idempotency request hash covers the delivery block.

`GET /customers/{id}/last-address` returns the recipient and address
fields from that customer's most recent delivery, or 404.

### 3.2 Editing before pickup

`PATCH /deliveries/{id}` (status `to_send` or `assigned`): any of the
recipient/address fields, `fee`, `driver_pay`, `collector`,
`pay_method`. Validation as at checkout. One `edit` event with the
changed fields.

A fee change posts a `fee_adjustment` transaction for the difference
(fee up: Dr `receivable[o]` · Cr `delivery_fee_income`; fee down: the
reverse). Requires an `Idempotency-Key`. A fee reduction that would take
the receivable below zero (customer already paid in full) is refused
with a field error asking to reduce the fee by refunding through
cancel/return instead; this keeps refunds in one place.

### 3.3 Cancel and check-in

Both reuse the 5b cancel path (`orders::cancel`): `sale_cancel` stock
movements for every line, `cancel` transaction reversing the sale
**including fee adjustments** (Dr `sales_revenue` goods, Dr
`delivery_fee_income` current fee, Cr `receivable[o]`), then a `refund`
per method paid. The 5b cancel function is extended to reverse
`delivery_fee_income` and is called from the delivery code inside the
same transaction.

- **Cancel** (`to_send`/`assigned`): delivery → `cancelled`, `closed_at` set, open assignment ended.
- **Check-in** (`failed`): body `{lines: [{order_line_id, disposition: sellable|damaged}], trip_pay?}`; every line must be listed. After the cancel, a `damage` movement (`source_type = 'delivery'`) per damaged line. Delivery → `returned`.
- Both require an `Idempotency-Key` and use the 5b retry pattern on the frontend.
- `stock_movements.source_type` CHECK adds `delivery`.

The 5b **Cancel sale** button on a delivery order calls the delivery
cancel; it is disabled with an explanation once the delivery is picked
up. The **Return** button on a delivery order is enabled only when the
delivery is `delivered`.

### 3.4 Driver access

- `GET /me/deliveries`: jobs where `current_driver_id = me` (active), plus the last 20 jobs this driver completed or failed (history: order number, locality, date, amount, status only).
- `GET /me/deliveries/{id}`: full detail (recipient, phone, address, landmark, instructions, item names and quantities, amount to collect, collector) **only while** the delivery is assigned to this driver and status is `assigned` or `picked_up`; history fields otherwise; 404 if this driver never had it.
- `POST /me/deliveries/{id}/status` `{to: picked_up|delivered|failed, received_by?, reason?, note?}`.
- "Amount to collect" = the order receivable when `collector = driver`, else ₹0.
- Drivers never see item prices, costs, other orders, customers, products or reports.

A user whose only role is `delivery_driver` lands on `/driver` after
login; every other route sends them back there.

### 3.5 Drivers

- `POST /drivers` `{display_name, username, password, phone, default_pay, note?}` creates a user with the `delivery_driver` role and a `driver_profiles` row (one transaction). Password rules as for the admin.
- `PATCH /drivers/{id}` edits name, phone, default pay, note, `is_active`. Deactivating ends the driver's sessions; their open jobs stay assigned and are flagged on the owner's Out list as "driver inactive" until reassigned.
- `POST /drivers/{id}/password` sets a new password and ends their sessions.
- All need `drivers.manage` (admin has it already).

## 4. API summary

| Purpose | Endpoint | Permission |
|---|---|---|
| Checkout with delivery | `POST /orders/checkout` + `delivery` | `orders.confirm` |
| List / detail | `GET /deliveries?status=`, `GET /deliveries/{id}` | `deliveries.read` |
| Assign / reassign | `POST /deliveries/{id}/assign` `{driver_id, driver_pay?}` | `deliveries.assign` |
| Edit before pickup | `PATCH /deliveries/{id}` | `deliveries.assign` |
| Status on driver's behalf | `POST /deliveries/{id}/status` | `deliveries.assign` |
| Retry | `POST /deliveries/{id}/retry` `{driver_id, driver_pay?, trip_pay?}` | `deliveries.assign` |
| Cancel | `POST /deliveries/{id}/cancel` | `deliveries.cancel` |
| Check in | `POST /deliveries/{id}/check-in` | `deliveries.check_in` |
| My jobs | `GET /me/deliveries`, `GET /me/deliveries/{id}` | `deliveries.read_own` |
| My status update | `POST /me/deliveries/{id}/status` | `deliveries.update_own` |
| Drivers | `GET/POST /drivers`, `PATCH /drivers/{id}`, `POST /drivers/{id}/password` | `drivers.manage` |
| Last address | `GET /customers/{id}/last-address` | `orders.read` |

`GET /deliveries` list items: id, order number, status, recipient
name, locality, driver name, driver active, amount to collect, fee,
age (time since created or since last status change), failed reason
for `failed`.

## 5. Screens

**Sell → Charge sheet.** Counter / Delivery switch. Delivery shows the
customer (required), recipient name and phone (prefilled from the
customer), address, locality, landmark, instructions (prefilled from
the last delivery), fee, driver pay (optional), "Who collects" (Driver /
Me) and, when the driver collects, "Driver's pay" (Keeps it from the
cash / I pay them). Summary: goods + delivery fee = total; "Driver
collects ₹X" or "Paid". After charging, the done screen offers the
WhatsApp receipt and "Assign driver".

**Deliveries** (new nav item, `deliveries.read`). Tabs **To send /
Out / Failed / Done** (Done = last 7 days of delivered, returned,
cancelled), with counts. Card: order number, recipient, locality,
driver (with "inactive" badge), amount to collect, age.

**Delivery detail.** Status and history; recipient with `tel:`; address
with a Google Maps search link; items; money (total, paid, still owed,
collector, driver pay). Actions by status: Assign / Reassign, Edit,
Cancel, Mark picked up / delivered / failed (on behalf), Retry, Check
in. WhatsApp: "Send to driver" and "Tell customer it's on the way".

**Sale detail.** A delivery panel (status, driver, link to the delivery);
Cancel and Return follow §3.3.

**Users.** "Add driver" form; driver rows show phone and default pay
with Edit, Reset password, Deactivate/Reactivate.

**Driver area `/driver`.** Own layout, no main nav. "My deliveries":
one card per active job (locality, amount to collect, status) and a
short history below. Detail: big Call and Map buttons, address,
instructions, items, amount to collect; one primary action for the
current step (Picked up → Delivered) and a secondary "Couldn't
deliver". Delivered asks "Who received it?"; Couldn't deliver asks for
the reason (and a note for Other). Refreshes on open and on
pull-to-refresh; a 404 shows "This job is no longer assigned to you"
and returns to the list.

## 6. WhatsApp texts

Built on the phone (`shared/whatsapp.ts`), opened with `wa.me`.

> **Note, 7 Nov 2026 (Ruling 10).** As built, the links are made on the
> server (`orders::whatsapp::wa_link`, like receipt links) and returned
> on the delivery detail as `driver_whatsapp_url` and
> `customer_whatsapp_url` (null when there is no driver). The texts
> below are unchanged; only where they are built differs.

To the driver:

```
*Delivery JJ-2026-27-00042*
Lalthangi · 9862000000
House 12, Chanmari West, Chanmari
Landmark: near the church
Note: call before coming
Map: https://www.google.com/maps/search/?api=1&query=<address, locality>
Collect: ₹1,050
Open: <PUBLIC_URL>/driver/deliveries/<id>
```

To the customer:

```
Your order JJ-2026-27-00042 from Jehovah Jireh is on its way with
<driver name> (<driver phone>). Amount to pay: ₹1,050.
```

"Amount to pay" is left out when nothing is owed.

## 7. Errors

| Case | Response |
|---|---|
| Transition not allowed from the current status | 409, message names the current status and what is allowed |
| Driver acts on a job not currently theirs | 404 |
| Missing `received_by`, reason, or note for `other` | 400 field error on that input |
| Assigning an inactive driver or a non-driver user | 400 field error on `driver_id` |
| `retain_from_collection` with `collector = shop` | 400 field error on `pay_method` |
| Fee reduction below what is already paid | 400 field error on `fee` |
| Check-in not listing every line | 400 field error on `lines` |
| Lost response on checkout / cancel / check-in / fee edit | same idempotency key → same result |

## 8. Testing

Backend integration tests (one per acceptance criterion at least),
plus: every delivery-related ledger transaction balances;
`delivery_fee_income` equals the sum of current fees on non-cancelled
delivery orders; the stock trace test
`every_unit_traces_to_a_confirmed_source` passes with check-in and
damage movements; concurrent assign/status calls on one delivery leave
one consistent result.

Frontend unit tests: delivery cart totals and request body; WhatsApp
texts; driver detail action rules per status; driver-only routing.

Manual check on the owner's phone and one driver's phone before
deploy.

## 9. Acceptance criteria (6a done)

1. A delivery order with a fee: total includes the fee, stock comes off once, the receivable includes the fee, and the fee is credited to `delivery_fee_income`.
2. Assigning a driver: the driver sees only their own active jobs, with full recipient details only while the job is theirs and active.
3. Reassigning removes the job from the previous driver at once (their next request gets 404).
4. Out-of-order transitions are rejected with 409.
5. Delivered requires a received-by name; failed requires a reason (and a note for `other`).
6. Retry works with the same or another driver; every failed attempt is kept with its trip pay.
7. Check-in cancels the order and refunds what was paid; only sellable lines are back in stock, damaged lines are written off.
8. Cancelling before pickup restores all stock and reverses the money including the fee.
9. Changing the fee before pickup corrects what is owed with a new posting; the original posting is unchanged.
10. Drivers can be added, have their password reset, and be deactivated/reactivated from the app; a deactivated driver cannot log in.
