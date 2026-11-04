# ADR-0002: Cloud hosting, online-only

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
The shop's internet connection is reliable. Drivers need to reach the
system from outside the shop. Offline-first POS needs a sync and
conflict model for stock and money, which is the hardest part of any
POS.

## Decision
Host on one cloud VPS in an Indian region. The app needs a connection
to make changes. The PWA caches the app shell and OCR model, and the
POS cart survives a reload. No sales are taken offline; a paper
fallback is documented ([Operations.md §7](../../Operations.md#7-staff-procedures-to-write)).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Cloud VPS, online-only** (chosen) | Simple; drivers can reach it; off-site by default | Shop can't sell through the app during an outage |
| Server in the shop | Works if the internet drops | Drivers need a tunnel; hardware, power and backups are the shop's problem |
| Offline-first PWA with sync | Sells during outages | Conflict resolution for stock and money; weeks of extra work and risk |

## Consequences
- Idempotency keys handle retries on flaky mobile connections.
- Outages are handled by alerting and the paper procedure.
- Revisit if outages become frequent enough to cost sales.
