# ADR-0004: Optional GST and gapless document numbering

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
The business is not GST-registered, but may register later. Switching
on tax afterwards shouldn't need a schema change or rewrite historical
documents. GST invoices need numbers that run in order with no gaps per
financial year.

## Decision
- `settings.tax_mode = none | gst`, default `none`; only an admin can
  change it, and it affects new orders only.
- Tax columns always exist on order lines and hold 0 when tax is off.
  Products have an optional HSN code and a tax category.
- Document numbers come from a `document_sequences` row per (type,
  financial year), updated within the confirming transaction. Format:
  `JJ/2026-27/000123`. Used from day one, whatever the tax mode.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Tax mode setting + gapless numbers now** (chosen) | No migration when registering; numbers already compliant | A little extra work now |
| Add GST later | Less now | Schema change, and old numbering may not be compliant |
| PostgreSQL `SEQUENCE` for numbers | No lock | Skips numbers after a rollback, so not gapless |

## Consequences
- Confirmations queue on the sequence row lock. At one shop's volume
  that's milliseconds.
- GST calculation rules (inclusive/exclusive, CGST/SGST/IGST) are built
  but must be checked with an accountant before `gst` is switched on.
