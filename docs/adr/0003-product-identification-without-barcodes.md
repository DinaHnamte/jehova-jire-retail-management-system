# ADR-0003: Product identification without barcodes

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
Ektra by Superware boxes have a printed text label with a model code
such as `B 6275-4.75`, and no barcode. The shop doesn't want to print
its own labels. Size variants share a base code (`B6275`).

## Decision
The printed product code is the main way products are identified.
- Normalise codes (uppercase, no spaces, standard hyphen) and split
  them into a base code and a variant.
- The POS finds products with fast search on code and name: exact →
  base code → digit prefix → trigram similarity. Results are grouped by
  base code.
- OCR has a **find** mode (code only) for the POS and goods receipt,
  and a **register** mode (full parse) for new products.
- Lookups from OCR text also try common OCR misreadings (`O/0`, `I/1`,
  `S/5`, `B/8`) and only ever suggest matches; a person confirms.
- The `barcode` identifier type is kept for future suppliers.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Code search + OCR** (chosen) | Works with the boxes as delivered; no extra labelling | Slower than barcode scanning; OCR accuracy has to be measured |
| Print internal barcode stickers | Fastest checkout; cheap scanners | Owner declined: labour per item and sticker cost |
| Name search only | Simplest | Many products have near-identical names |

## Consequences
- The normaliser and search are core. They get the most tests and run
  against a real label photo corpus.
- POS is designed keyboard-first, with recent and best-selling items.
- Revisit if checkout speed becomes a complaint.
