# Phase 5: Selling, customers and money (design)

**Date:** 6 Nov 2026
**Implements:** Architecture.md §10 (Orders and payments), §11, §20 Phase 5
**Status:** approved in chat on 6 Nov 2026; awaiting review of this written version

## Goal

Record every sale on the phone, take stock off, know who has paid and
who still owes, and send the customer a receipt on WhatsApp. At the end
of a day the sales add up to the cash and UPI received plus what
customers newly owe.

**Exit criterion (from §20, adjusted):** concurrency and idempotency
tests pass; a day's sales reconcile to cash + UPI received + new
amounts owed.

## Decisions made with the owner (6 Nov 2026)

| Question | Answer |
|---|---|
| Receipt printer? | **None.** Receipts go to the customer on WhatsApp. |
| Who sells? Till open/close? | **One person**, selling from home stock (not a shop front). **No till, no cash counting, no shifts.** |
| WhatsApp method | **Click-to-chat link** (`wa.me`) opened on the owner's own phone; the owner presses send. No WhatsApp Business API. |
| Remember customers? | **Yes.** A customer is saved by phone number (+ optional name) and reused for later sales and Phase 6 deliveries. |
| Payment | Cash, UPI, or both. **Pay later and part payments are allowed** (customer credit, previously deferred in §10.6). |
| Discounts / price edits / cancel / returns | **All needed.** Reasons are optional (one user). |
| Tax | Stays `tax_mode = none`. Document title is "Receipt". |

## Split into two deliveries

One spec, two implementation plans, each deployed on its own:

- **5a — Sell:** customers, checkout, document numbers, money ledger,
  pay later and later payments, discounts and price edits, WhatsApp
  receipt and public receipt page, sales list, daily summary.
- **5b — Undo:** cancel a sale, returns (sellable / damaged) with a
  cash/UPI refund or a reduction of what's owed.

Until 5b ships, a mistaken sale is corrected with a stock adjustment
and a note.

## Not in this phase

- Printing (ESC/POS or browser print)
- WhatsApp Business API / automatic sending
- Drafts and held carts on the server (the phone keeps the cart)
- Advance payments or customer credit balances above zero (overpaying is refused)
- Profit and margin reports (some opening stock has no cost yet)
- GST (`tax_mode = gst` stays unbuilt beyond the zero-filled columns)
- Delivery orders (Phase 6)
- Supplier bills and payments

## Changes to Architecture.md

- §10.1: no stored `completed` status. `orders.status` is `confirmed`
  or `cancelled`; paid / partly paid / unpaid is calculated from the
  ledger. Drafts are not stored. Phase 6 may add delivery states.
- §10.6: customer credit is allowed for all sales.
- §10.7: receipts are a WhatsApp text plus a public receipt page, not
  `window.print()`.
- Ledger accounts in this phase: `receivable[order]`, `cash`, `upi`,
  `sales_revenue`, `sales_returns`. (`cash_till` is renamed `cash`;
  `upi_bank` is renamed `upi`; there is no till.)
- §21: the printer and cashier questions are answered.

## 1. Data model

New migration(s), same conventions as Phases 2 and 4: UUID v7 ids made
in Rust, `TIMESTAMPTZ`, money `NUMERIC(12,2)`, `ON DELETE RESTRICT`.

**`customers`**
- `id`
- `phone TEXT NOT NULL UNIQUE`: normalised to 10 digits (Indian mobile);
  input with `+91`, `0`, spaces or dashes is accepted and normalised
- `name TEXT`: optional, ≤ 120 characters
- `note TEXT`: optional, ≤ 500 characters
- `created_at`, `updated_at`

**`document_sequences`**: `(doc_type, fiscal_year)` primary key,
`next_value BIGINT`. Fiscal year runs 1 April – 31 March in
Asia/Kolkata; label `2026-27`. Taken with `UPDATE … RETURNING` inside
the checkout transaction (ADR-0004). Format `JJ/2026-27/000042`.

**`orders`**
- `id`, `number TEXT UNIQUE NOT NULL`
- `customer_id` (nullable; required when anything is left unpaid)
- `fulfilment TEXT NOT NULL DEFAULT 'counter'` (check: `counter` only for now)
- `status TEXT NOT NULL CHECK (status IN ('confirmed','cancelled'))`
- `subtotal` (sum of lines at the price actually charged, before order discount),
  `order_discount` (≥ 0), `total` (= subtotal − order_discount, ≥ 0)
- `note`, `receipt_token TEXT UNIQUE NOT NULL` (128-bit random, URL-safe)
- `created_by`, `created_at`
- `cancelled_at`, `cancelled_by`, `cancel_reason` (5b)

**`order_lines`**
- `id`, `order_id`, `line_no`
- `product_id`, snapshot `product_name`, `product_code`
- `quantity BIGINT > 0`
- `list_price`: the product's price at the time
- `unit_price`: price actually charged per unit (may differ from list)
- `line_discount` ≥ 0: line-level money off
- `order_discount_share` ≥ 0: this line's part of the order discount
- `net_amount` = `quantity × unit_price − line_discount − order_discount_share`, ≥ 0
- `tax_rate`, `tax_amount`: always 0 in this phase
- `unit_cost`: weighted average cost at sale (nullable when unknown)
- `returned_quantity` is **not** stored; it's summed from `return_lines` (5b)

**`ledger_transactions`, `ledger_entries`**: as Architecture.md §14.
`kind` values this phase: `sale`, `payment`, `cancel` (5b), `refund`
(5b), `return_credit` (5b). A deferred constraint trigger enforces
sum = 0 per transaction. Rows are never updated or deleted.

**`returns`, `return_lines`** (5b): return header (order, who, when,
refund method or "reduce owed", note); lines (order line, quantity,
disposition `sellable | damaged`, amount).

**Stock movement types** added to the check: `sale`, `sale_cancel`,
`sale_return`. Source type `order` / `return`.

## 2. Money postings

Debits positive, credits negative.

| Event | Entries |
|---|---|
| Sale, total T | Dr `receivable[o]` T · Cr `sales_revenue` T |
| Paid now: cash C, UPI U | (one `payment` txn) Dr `cash` C · Dr `upi` U · Cr `receivable[o]` C+U |
| Later payment P from a customer | (one `payment` txn) Dr `cash` or `upi` P · Cr `receivable[oᵢ]`, applied oldest sale first until P is used |
| Cancel sale (5b) | Reverse the sale txn; reverse each payment txn on that order (`reverses_id`), which records the paid money as handed back by the same method. Stock comes back via `sale_cancel` |
| Return with refund (5b) | Dr `sales_returns` A · Cr `cash`/`upi` A |
| Return reducing what's owed (5b) | Dr `sales_returns` A · Cr `receivable[o]` A |

Rules:
- A checkout payment can't exceed the total; a later payment can't
  exceed what the customer owes. There are no negative receivables.
- A sale with anything left unpaid must have a customer.
- Customer owes = Σ `receivable` entries over that customer's orders.
- Order paid status: owed = 0 → paid; 0 < owed < total → partly paid;
  owed = total → unpaid.
- Returns (5b): the amount for a returned line is
  `net_amount × returned_qty / quantity`, rounded to paise. A return on
  an order that still has money owed reduces what's owed first, then
  refunds the rest.

## 3. Checkout

`POST /api/v1/orders/checkout`, with an `Idempotency-Key` header.

Request: lines `[{product_id, quantity, unit_price?, line_discount?}]`,
`order_discount?`, `customer_id?` or `new_customer {phone, name?}`,
`payments [{method: cash|upi, amount}]`, `note?`.

In one transaction:
1. Check the idempotency key (same key + same body → original result;
   different body → 409).
2. Load products; `list_price` from the catalogue; `unit_price`
   defaults to `list_price`. Validate prices ≥ 0 and discounts ≤ line
   amount. The client's totals are never trusted.
3. Spread `order_discount` across lines by net value (largest-remainder
   to the paise, so the shares add up exactly).
4. Create or match the customer (by normalised phone).
5. Take the document number.
6. Lock stock balances in product-id order; refuse with 422
   `INSUFFICIENT_STOCK` naming the product if any goes below 0. Insert
   `sale` movements and snapshot `unit_cost`.
7. Insert the order and lines, then post `sale` and (if any money is
   paid) `payment`.

Response: the order with lines, paid, owed, receipt URL, and WhatsApp
text.

Other endpoints (5a):
- `GET /orders?from=&to=&status=&paid=` (paged), `GET /orders/{id}`
- `POST /customers`, `GET /customers?q=` (phone prefix or name),
  `GET/PATCH /customers/{id}` (includes owes, sales, payments)
- `POST /customers/{id}/payments` (Idempotency-Key): method, amount, note
- `GET /reports/daily?date=`
- `GET /public/receipts/{token}` (no auth, rate-limited)

5b: `POST /orders/{id}/cancel`, `POST /returns`.

Permissions use the existing codes: `orders.read`, `orders.confirm`
(checkout), `payments.record`, `orders.void` (cancel),
`returns.create`, `reports.read`. Customers use `orders.read` /
`orders.confirm`; no new permission codes.

## 4. Screens

**POS (`/pos`)**
- Search box (same search as products; 120 ms debounce, stale requests
  aborted). Tap a result to add it; tap again for +1.
- Cart lines: quantity −/+, price (tap to edit), "₹ off" / "% off".
  The usual price is shown struck through when changed. There's a
  whole-sale discount field ("₹ off" or "round to ₹…").
- Customer: type phone or name to pick, or add new (phone, name). Shows
  "owes ₹X" when they have a balance.
- Charge sheet: cash and UPI amounts, defaulting to full total in cash;
  "Pay later" sets both to 0. Shows the amount left owing. Requires a
  customer if anything is left owing.
- Done screen: number, total, paid, owed; **Send on WhatsApp** (when
  there's a phone), **New sale**.
- The cart is kept in Pinia + `localStorage`. The Idempotency-Key is
  generated when the charge sheet opens and reused on retry.

**Sales (`/sales`)**: list by date with a paid/partly/unpaid filter.
The detail view shows the receipt and **Send on WhatsApp** and **Record
payment**. In 5b it also gets **Cancel sale** and **Return items**.

**Customers (`/customers`)**: search; detail shows owes, sales,
payments, **Record payment**, and **Remind on WhatsApp**.

**Overview**: today's summary card (see §6). Tap it for other dates.

**Public receipt (`/r/:token`)**: a standalone page with no app chrome
and no login. Shows the shop name, number, date, lines, discount,
total, paid, owed, or a "CANCELLED" banner.

## 5. WhatsApp

Link: `https://wa.me/91<10 digits>?text=<url-encoded>`. It opens in
WhatsApp on the owner's phone.

Receipt text (WhatsApp `*bold*`):

```
*Jehovah Jireh* — Receipt JJ/2026-27/000042
6 Nov 2026

2 × IP 1060-11" — ₹900
1 × B 6143-8 MP4952 — ₹350
Discount — −₹50
*Total ₹1,200*
Paid ₹800 (cash) · *Due ₹400*

View: https://jehovah-jire.hnamtek.in/r/<token>
```

- At most 15 lines, then "+N more items — see link".
- "Paid in full" replaces the Due part when owed = 0.
- Reminder text: "Hello <name>, you have ₹450 due with Jehovah Jireh
  for receipts JJ/…/000042, JJ/…/000051. Thank you!"
- The text is built on the server (one tested function) and returned
  with the order and the customer, so the screens just open the link.
- The shop name comes from the `settings` table key `shop_name`
  (seeded "Jehovah Jireh").

## 6. Daily summary

For one date (Asia/Kolkata):
- number of sales, sales total (Σ order totals), discounts given
  (Σ quantity × list_price − Σ order totals)
- cash received, UPI received (all payments that day, including later payments)
- new amount owed (sales total − payments against that day's sales)
- total owed by all customers right now
- 5b: cancellations and returns/refunds that day

Each figure states how it's calculated, in small text. Profit isn't shown.

## 7. Errors

- Standard error shape (§15). `INSUFFICIENT_STOCK` (422) names the product
  and quantity left. `CUSTOMER_REQUIRED` (422) when money is left
  owing. `OVERPAYMENT` (422). `IDEMPOTENCY_CONFLICT` (409).
- A failed charge leaves the cart untouched; the charge sheet shows the
  message and the same key retries.
- Public receipt: an unknown token is a plain 404 page, rate-limited by IP.

## 8. Testing

Integration (real Postgres, as in Phase 4):
- checkout writes stock movements, ledger, number and order together;
  a forced failure part-way leaves nothing
- a retried checkout with the same key makes one sale; a different body → 409
- two concurrent checkouts for the last unit: exactly one succeeds
- concurrent checkouts produce gapless numbers
- every ledger transaction sums to zero (constraint trigger)
- a later payment is applied oldest sale first and can't overpay
- customer owes and order paid status match the ledger
- the public receipt is reachable by token only and shows no cost
- the nightly check gets one more job: account balances = Σ entries
- 5b: cancel reverses exactly the sale's stock and money; return with
  refund vs reduce-owed; damaged items don't return to stock; can't
  return more than sold

Unit: discount spreading and paise rounding, phone normalisation,
fiscal year label, WhatsApp text (truncation, paid-in-full, due).

End-to-end (Playwright): search → cart → edit price → pay part → done
screen shows the correct wa.me link → record later payment → owes 0.
