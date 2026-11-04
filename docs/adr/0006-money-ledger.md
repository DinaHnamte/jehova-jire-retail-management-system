# ADR-0006: Double-entry money ledger

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
Money passes through several hands: counter cash, UPI to the shop,
drivers collecting at the door, drivers keeping their pay, the shop
paying drivers, partial payments, refunds. v1 proposed about seven
tables with status fields (remittances, compensations, settlement
batches, …) and several overlapping enums. They contradicted each
other, and the status fields could drift out of step with each other.

## Decision
Record every money event as a `ledger_transactions` row with
`ledger_entries` that sum to zero (debits positive, credits negative).
Accounts include `receivable[order]`, `cash_till`, `upi_bank`,
`sales_revenue`, `delivery_fee_income`, `tax_payable`,
`driver_custody[driver]`, `driver_payable[driver]`,
`delivery_expense`, `sales_returns` and `cash_shortage`. Payment
status, outstanding amounts and driver settlement are all **calculated
from entries**. Corrections are reversing transactions.

Driver pay is always posted as money the shop owes the driver. A driver
keeping their pay out of collections is a separate posting
(`driver_payable` → `driver_custody`), never netted silently.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Double-entry ledger** (chosen) | One mechanism for every flow; balances can't disagree; full audit trail; partial amounts come free | Postings must be built correctly (unit-tested builders + a sum-to-zero constraint) |
| Status fields on orders and deliveries | Quick to start | Drifts; can't explain *why* a number is what it is; every new flow needs new fields |
| Separate tables per flow (v1) | Each flow is explicit | Many tables, overlapping states, hard to reconcile across them |

## Consequences
- The cash position report is a simple query.
- Not a full accounting system: no chart of accounts, period closing or
  GST returns. An accounting export can be built from it later.
