# ADR-0005: One order model; stock deducted at confirmation

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
v1 treated a sale as one step at checkout (paid and done). Delivery
orders are confirmed now, may be paid later by a driver or in parts,
and can fail and come back. Two separate models (`sales` and
`delivery_orders`) would duplicate pricing, snapshots, numbering and
returns.

## Decision
One `orders` table with `fulfilment = counter | delivery` and status
`draft → confirmed → completed | cancelled`, plus `voided`. Payment
status is calculated from the money ledger. Stock is deducted at
**confirmation** for both types. A counter checkout is one API call:
create, confirm and pay in one transaction.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **One order model, deduct at confirm** (chosen) | One set of pricing, snapshot and return rules; goods set aside for delivery orders can't be sold at the counter | Cancelling a delivery order needs a reversing stock movement |
| Deduct at dispatch/pickup | Stock reflects what's physically on the shelf | Needs reservations, or the same unit can be sold twice |
| Separate sales and delivery-order tables | Each simple alone | Duplicated logic; reports must join both |

## Consequences
- Cancellation and failed delivery produce `order_cancel` or
  `delivery_return` movements. History is never edited.
- Customer credit for counter sales becomes a permission and UI change
  later, because the ledger already supports an unpaid balance.

## Update, 6 Nov 2026 (Phase 5a)
Built differently from the text above: `orders.status` is only
`confirmed` or `cancelled`. There is no stored `draft` or `completed`;
paid, partly paid and unpaid are calculated from the money ledger. The
rest of the decision stands (one table, stock out at confirmation, one
checkout call). Customer credit is allowed for all sales, as the last
consequence anticipated.
