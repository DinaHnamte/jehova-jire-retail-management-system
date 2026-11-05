# Label photo corpus

Real Ektra by Superware box labels, used to measure the scanner
(Architecture.md §7.3). Each `<name>.jpg` has a `<name>.json` with the
values a person reads off the label.

- Images here are resized to 2048 px on the long edge, about what a phone
  camera stream gives the scanner. Full-size originals are kept in
  `originals/` locally and are not committed.
- `code` is the normalised model code: no spaces, no inch mark. It is the
  product's identity.
- `design_code` (e.g. `MP4952` pink, `MG4705` green) is the print or
  colour, and part of the product's identity: each design is its own
  product (ADR-0003). The parser must read it as the design, never as the
  model code. `design_name` ("Retro Daisy") stays in the product name.
- `mrp` is in rupees; `null` where the label has no price.
- `mfg_date` is the batch month, never used to identify a product.

Name files `<code>-<what's special>.jpg`. Add awkward photos on purpose:
blurry, dim, rotated, partly covered.
