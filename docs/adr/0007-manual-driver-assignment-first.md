# ADR-0007: Manual driver assignment before an offer board

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
1–3 deliveries a day at launch, expected to grow. v1 specified a
part-time driver pool with self-registration, document verification,
availability, an offer board with atomic job claiming, and settlement
batches. That's a gig-platform feature set, and it also needs a
notification channel v1 didn't specify.

## Decision
Phase 1: a manager assigns each delivery to a known driver. Drivers use
a minimal `/driver` area of the PWA. WhatsApp and `tel:` links replace
notifications. Phase 2 (driver pool, offer board, Web Push) is outlined
in [Delivery.md §11](../../Delivery.md#11-later-phase-driver-pool-and-offer-board)
and adds tables without changing phase 1 tables.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Manual assignment first** (chosen) | Ships with the POS; fits current volume | Manager does the dispatching |
| Offer board from day one | Ready for growth | Weeks of work and a notification system for 1–3 jobs a day |

## Consequences
- Triggers to start phase 2: about 10+ deliveries a day, more drivers
  than the manager can coordinate by phone, or dispatch delays
  customers notice.
