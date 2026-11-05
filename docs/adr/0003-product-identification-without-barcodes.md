# ADR-0003: Product identification without barcodes

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
Ektra by Superware boxes have a printed text label with a model code
such as `B 6275-4.75`, and no barcode. The shop doesn't want to print
its own labels. Size variants share a base code (`B6275`).

Photos of real labels (5 November 2026, [`fixtures/labels/`](../../fixtures/labels/README.md))
showed:
- **Two label types.** A *plain* label (description and codes in large
  type) and a *price* label (the same codes in small type, plus MRP,
  "Inclusive of All Taxes", Mfg. date and the maker's address). A box
  may have both, or only the price label.
- **The model code is on every label** and always looks like letters,
  four digits, then a size: `IP 1060-11"`, `IB 1290 -7.8"`,
  `B 6143-8`. The prefix varies (`B`, `IB`, `IP`, …), and so does the
  spacing around the hyphen.
- **Some labels add a design code or name** after the model code:
  `MP4952` (pink) and `MG4705` (green) on the same plate, or
  `Retro Daisy` on a lunch box. Others have none.
- Some carry a sub-brand line, e.g. `Beuniq (B 6143-8)`.

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
- **The model code alone identifies a product.** Designs and colours
  (`MP4952`, `MG4705`, `Retro Daisy`) are not tracked: every design of
  a model is one product with one stock count. The parser recognises a
  design code so it is never mistaken for the model code, then drops it.
- Either label type can be scanned. When a price label is read, its
  MRP pre-fills a new product, and in find mode it is compared with
  the saved MRP; a difference is shown as a warning (old stock or a
  price change). **The till always charges the saved price**, never the
  scanned one. Mfg. date is ignored.

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
- Stock can't say how many of each colour are left. Revisit if
  customers often ask for a particular design; designs would become
  product variants.
- Revisit if checkout speed becomes a complaint.
