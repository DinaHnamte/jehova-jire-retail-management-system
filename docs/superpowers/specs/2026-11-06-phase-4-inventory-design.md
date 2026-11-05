# Phase 4: Inventory and receiving goods (design)

**Date:** 6 Nov 2026
**Implements:** Architecture.md §8 (Inventory), §9 (Purchasing), §20 Phase 4
**Status:** approved in chat on 6 Nov 2026; awaiting review of this written version

## Goal

Know how many of each product the shop has, and where every unit came
from. Staff count what's on the shelves once at go-live, then record each
supplier delivery. The app keeps an average cost per product so that
Phase 5 can report profit.

**Exit criterion (from §20):** every unit on hand can be traced to a
goods receipt or a stock count.

## Decisions made with the owner (6 Nov 2026)

| Question | Answer |
|---|---|
| Where is stock kept? | **The shop only.** There is one location. The table exists, but the screens never show it. |
| How does stock arrive? | **Staff just receive it.** Goods come with a supplier bill and staff enter what came in and its cost. No purchase orders. |
| Track what's owed to suppliers? | **Later.** This phase records goods and cost only. Supplier bills and payments come with the money ledger in Phase 5. |
| Who changes stock? | **Admin and manager**, through the permissions that already exist. The *Stock operator* role can receive and count. A cashier can only view. |
| Cost of opening stock | **Entered when known.** Otherwise it is blank ("cost unknown") until the next delivery of that product sets it. |
| Opening count compared with recounts | **One stock-count flow does both.** A product's first count becomes its opening balance. A later count becomes an adjustment. |

## Not in this phase

- Purchase orders
- Supplier bills, payments and balances
- Returns to suppliers
- Transfers, and more than one location in the screens
- Low-stock alerts and reorder levels
- Sales movements (Phase 5)
- Scanning (Phase 3, parked)

## 1. Data model

New migration. It follows the catalogue conventions: UUID v7 ids made in
Rust, `TIMESTAMPTZ`, and money as `NUMERIC`.

**`locations`**: `id`, `code` (unique), `name`, `created_at`. One row is
seeded with a fixed id, `00000000-0000-7000-8002-000000000001`, code
`shop`, name "Shop".

**`suppliers`**:
- `id`
- `name`: 1–120 characters; unique case-insensitively among active suppliers
- `phone`: optional
- `notes`: optional
- `archived_at`, `created_at`, `updated_at`

**`stock_movements`**: these rows can never be changed.
- `id`, `product_id`, `location_id`
- `quantity BIGINT`: non-zero and signed
- `unit_cost NUMERIC(12,2)`: optional; filled in for movements that bring stock in when the cost is known
- `movement_type`: one of `opening_balance`, `purchase_receipt`, `adjustment_in`, `adjustment_out`, `damage`. Phase 5 adds sale types to this check.
- `source_type`: one of `goods_receipt`, `stock_count`, `adjustment`
- `source_id UUID`
- `reason TEXT`: required for `adjustment_*` and `damage`
- `created_by`, `created_at`
- Index on `(product_id, created_at DESC)`
- A trigger raises an error on `UPDATE` or `DELETE`.
- The quantity's sign must match the type: `opening_balance`, `purchase_receipt` and `adjustment_in` are positive; `adjustment_out` and `damage` are negative.

**`stock_balances`**: a cache kept in step with the movements.
- `product_id`, `location_id` (primary key together)
- `on_hand BIGINT NOT NULL CHECK (on_hand >= 0)`
- `avg_cost NUMERIC(12,4)`: blank means cost unknown
- `updated_at`
- A product with no row has 0 on hand and an unknown cost.

**`goods_receipts`**:
- `id`, `supplier_id`, `received_on DATE`, `note`
- `status`: `draft` or `confirmed`
- `confirmed_by`, `confirmed_at`, `created_by`, `created_at`, `updated_at`

**`goods_receipt_lines`**:
- `id`, `receipt_id`, `product_id`
- `quantity BIGINT > 0`
- `unit_cost NUMERIC(12,2) >= 0`: required on a receipt
- Unique `(receipt_id, product_id)`

**`stock_counts`**:
- `id`, `note`
- `status`: `draft` or `posted`
- `posted_by`, `posted_at`, `created_by`, `created_at`, `updated_at`

**`stock_count_lines`**:
- `id`, `count_id`, `product_id`
- `counted BIGINT >= 0`
- `unit_cost NUMERIC(12,2)`: optional; only used when the line becomes an opening balance
- When posted, the line also stores `expected BIGINT`, the quantity on hand at that moment, so the difference can be shown later.
- Unique `(count_id, product_id)`

**`stock_check_runs`**:
- `id`, `ran_at`
- `mismatches JSONB`: a list of `{product_id, balance, ledger_sum}`; an empty list means everything matched

## 2. Rules (each one gets a test)

1. **One write path.** Every change to stock goes through a single
   function. Inside the caller's transaction it locks the balance rows in
   `product_id` order, writes the movements, updates the balances and
   returns the new balances. Nothing else writes to `stock_balances`.
2. **Confirming a receipt.**
   - The receipt must be a draft with at least one line, and the supplier must be active.
   - Each line becomes a `purchase_receipt` movement at its cost.
   - The average cost becomes `(on_hand × avg + qty × cost) ÷ (on_hand + qty)`, rounded to 4 decimal places.
   - When the cost is unknown, or there was no stock, the line's cost becomes the average.
   - The status changes to `confirmed` in the same transaction.
   - Confirming again returns **409 ALREADY_CONFIRMED** and changes nothing. If two people confirm at the same time, exactly one succeeds.
3. **Confirmed receipts can't be changed.** A mistake is corrected with
   an adjustment. Drafts can be edited and deleted.
4. **Posting a count.**
   - For each line, the difference is `counted − on_hand` at the moment of posting, read under the lock.
   - **If the product has no movements yet**, the line becomes `opening_balance` (+counted, when counted > 0). Its `unit_cost`, if given, becomes the average cost.
   - **Otherwise**, a positive difference becomes `adjustment_in` and a negative one `adjustment_out`, with the reason "Stock count". A difference of zero writes no movement.
   - `expected` is stored on the line, and the count becomes `posted`. Posting again returns 409 ALREADY_POSTED.
5. **Adjustments.**
   - Types are `adjustment_in`, `adjustment_out` or `damage`, with a quantity above 0 and a reason of 3–200 characters.
   - Stock can't go below zero: the request is refused with 422 INSUFFICIENT_STOCK, which includes the quantity on hand.
   - Adjusting in leaves the average cost unchanged. If the cost was unknown it stays unknown.
6. **Archived products** can't be added to new receipts or counts. Their
   history and stock stay visible.
7. **Nightly stock check** runs at 02:00 Asia/Kolkata inside the API
   process.
   - For every product it compares `stock_balances.on_hand` with `SUM(quantity)` from the movements, and stores one `stock_check_runs` row.
   - It never corrects anything. A mismatch is logged at error level and shown to anyone with `inventory.read`.
   - Admins can also start a run by hand.

## 3. Permissions (existing codes, no new ones)

| Action | Permission |
|---|---|
| View stock, movements, stock-check results | `inventory.read` |
| View receipts and suppliers | `purchases.read` |
| Create, edit and confirm receipts; add and edit suppliers | `purchases.receive` |
| Create and post stock counts | `inventory.count` |
| Adjustments | `inventory.adjust` |
| Run the stock check by hand | `inventory.adjust` |

Every confirm, post and adjustment writes to `audit_logs`.

## 4. API (under `/api/v1`, standard error shape, paged lists)

- `GET /stock`
  - Paged.
  - Search uses the catalogue's search.
  - Filters: `filter=out|unknown_cost`.
- `GET /products/:id/stock` returns the balance and average cost.
- `GET /products/:id/movements` is paged.
- `POST /products/:id/adjustments`
- `GET|POST /suppliers`, `GET|PATCH /suppliers/:id`
- `GET|POST /goods-receipts`
- `GET|PATCH|DELETE /goods-receipts/:id`
- `PUT /goods-receipts/:id/lines` replaces all of a draft's lines.
- `POST /goods-receipts/:id/confirm`
- `GET|POST /stock-counts`, `GET|PATCH|DELETE /stock-counts/:id`
- `PUT /stock-counts/:id/lines`
- `POST /stock-counts/:id/post`
- `GET /stock-checks/latest`, `POST /stock-checks`

## 5. Screens

**Navigation:** a new **Stock** section, plus **Receive goods** and
**Suppliers** under it. The bottom tabs on phones stay at four or fewer.

- **Stock list**
  - Paged, with search.
  - Each row shows the code, name, on-hand quantity and average cost. A blank cost shows a "Cost unknown" golden badge.
  - Filter chips: *Out of stock*, *Cost unknown*.
  - A banner appears if the last stock check found mismatches.
- **Product page** gets a stock card with:
  - on-hand quantity and average cost;
  - an **Adjust** button (shown only to people allowed to adjust);
  - the movement history.
- **Receive goods**
  - The list shows drafts first, then confirmed receipts.
  - The editor:
    - Supplier picker, with *Add supplier* right there.
    - The date, defaulting to today.
    - Lines are added through product search by code.
    - Each line has its quantity, its cost (pre-filled from the product's `default_cost`) and its total, with the grand total below.
    - **Confirm receipt** asks the person to confirm first.
- **Stock count**
  - Start a count and add products by search, entering the counted quantity and, optionally, the cost.
  - **Review** shows the expected quantity, the counted quantity and the difference for each product.
  - **Post count**.
- **Suppliers**: a list plus an edit form.

## 6. Testing

- **Backend integration tests** against Postgres:
  - each rule in §2;
  - the average-cost arithmetic, with a table of cases;
  - double confirm and concurrent confirm, using two connections;
  - the zero floor;
  - the trigger blocking `UPDATE` and `DELETE` on movements;
  - the stock check catching a mismatch planted with direct SQL;
  - permissions: a cashier gets 403 on each write.
- **Exit-criterion test.** A mixed scenario of opening count, receipts,
  adjustments and a recount. After it, for every product,
  `SUM(movements) = on_hand`, and every movement's source document exists
  and has been confirmed or posted.
- **Frontend unit tests** for the line-total and grand-total arithmetic
  and the request builders.

## 7. Known limits (accepted)

- **Receipts confirmed while a count is in progress.** If a receipt is
  confirmed between counting the shelf and posting the count, posting
  undoes that receipt's quantity. The Review screen flags any product
  that has a movement newer than the count's `created_at`. Staff should
  post counts promptly.
- **Cost of stock adjusted in.** Stock adjusted in is valued at the
  current average cost.
