# Jehovah Jireh --- System Architecture

**Document:** `Architecture.md`\
**Product:** Jehovah Jireh Retail Management System (JJ Retail)\
**Status:** Proposed architecture\
**Version:** 1.0\
**Last updated:** 5 November 2026

------------------------------------------------------------------------

## 1. Purpose

Jehovah Jireh is a retail management system for operating a
product-based retail business. It combines product cataloguing,
label-based product recognition, inventory control, purchasing, point of
sale (POS), customer and supplier records, and operational reporting.

The initial use case includes Ektra by Superware products whose
packaging commonly uses printed text labels rather than barcodes. The
application must therefore support camera-based label scanning and
deterministic text extraction. OCR is an input method, not the source of
truth: a person must be able to review and correct extracted data before
a product or stock transaction is committed.

This document defines the proposed system boundaries, components, data
model, workflows, APIs, security controls, deployment approach, and
implementation sequence.

## 2. Product goals

### 2.1 Primary goals

1.  Maintain a reliable, searchable product catalogue.
2.  Register products from printed labels using a browser camera or
    uploaded image.
3.  Receive stock from suppliers and maintain accurate on-hand
    quantities.
4.  Sell products through a fast POS workflow.
5.  Preserve an auditable history of every stock movement.
6.  Track purchase costs, selling prices, discounts, returns, and
    payment methods.
7.  Provide useful operational reports without compromising
    transactional accuracy.
8.  Keep the core application maintainable and suitable for a small
    retail operation, while allowing future expansion.

### 2.2 Non-goals for the initial release

The first release should not attempt to become a full accounting,
payroll, e-commerce, ERP, or supplier marketplace platform. It should
not automatically trust OCR output, infer unknown product facts, or
silently modify stock. Advanced demand forecasting, loyalty programmes,
multi-warehouse optimisation, and integrations with external accounting
systems can be considered later.

### 2.3 Architectural principles

-   **Correctness before convenience:** inventory and payment changes
    must be transactional and auditable.
-   **Catalogue data is canonical:** OCR and user input propose data;
    the persisted product record is authoritative.
-   **Ledger-based stock:** stock changes are represented by immutable
    movements, not unexplained edits to a quantity field.
-   **Explicit business operations:** receiving, selling, returning,
    adjusting, and transferring stock are separate operations.
-   **Thin frontend, strong backend:** the browser handles interaction
    and OCR; the Rust API enforces business rules.
-   **No LLM dependency:** text extraction and product matching use OCR,
    normalization, deterministic parsing, and database search.
-   **Modular monolith first:** one backend deployment with clear
    internal modules is simpler than premature microservices.
-   **Secure by default:** authentication, authorization, validation,
    audit logging, and backups are core features.

## 3. Scope and assumptions

This architecture assumes:

-   The first deployment serves one retail business at one primary
    store.
-   The system is initially used through a web application on desktop
    and mobile browsers.
-   The frontend is Vue 3 with TypeScript.
-   The backend is Rust using Axum and Tokio.
-   PostgreSQL is the primary relational database.
-   OCR runs in the browser using PaddleOCR.js where supported.
-   Product labels are printed and generally contain a product
    description, dimensions or size, and sometimes a model/product code.
-   A user confirms product details and stock quantities before saving.
-   The application may later support multiple stores or businesses, but
    multi-tenancy is not required to validate the first release.

If multiple legal businesses will use the same installation, tenant
isolation must be designed and tested before onboarding them. Do not add
a `tenant_id` column casually after production data exists.

## 4. System context

``` mermaid
flowchart TD
    Staff[Cashiers and Staff]
    Manager[Managers and Administrators]
    Browser[Vue Web Application]
    OCR[Browser OCR Worker]
    API[Rust API - Axum]
    DB[(PostgreSQL)]
    Files[Object/File Storage]
    Backup[Off-site Backups]

    Staff --> Browser
    Manager --> Browser
    Browser --> OCR
    OCR --> Browser
    Browser -->|HTTPS JSON API| API
    API --> DB
    API --> Files
    DB --> Backup
    Files --> Backup
```

### 4.1 Main actors

  ---------------------------------------------------------------------
  Actor                              Typical responsibilities
  ---------------------------------- ----------------------------------
  Administrator                      Configure business, users, roles,
                                     tax settings, and system
                                     preferences

  Manager                            Manage products, purchases, stock
                                     adjustments, reports, and returns

  Cashier                            Search products, create sales,
                                     accept payments, and issue
                                     receipts

  Stock operator                     Scan/register products, receive
                                     stock, and perform approved stock
                                     operations

  Auditor/read-only user             Review transactions, stock
                                     movements, and reports
  ---------------------------------------------------------------------

A single person may hold multiple roles. Permissions should be assigned
to capabilities rather than hard-coded around job titles.

## 5. High-level technology choices

  -----------------------------------------------------------------------
  Layer                   Proposed technology     Responsibility
  ----------------------- ----------------------- -----------------------
  Frontend                Vue 3 + TypeScript      User interface and
                                                  application workflows

  Build tooling           Vite                    Development server and
                                                  production assets

  UI styling              Tailwind CSS or a       Responsive interface
                          consistent component    
                          system                  

  Client state            Pinia where shared      UI state
                          state is justified;     
                          composables for local   
                          reusable logic          

  Camera                  MediaDevices API        Capture product labels

  OCR                     PaddleOCR.js            Browser-side text
                                                  detection and
                                                  recognition

  Image processing        Browser Canvas          Crop, rotate,
                          initially; OpenCV.js    perspective correction,
                          only if needed          contrast

  Barcode (future)        ZXing                   Optional barcode
                                                  recognition

  Backend                 Rust + Axum + Tokio     HTTP API and business
                                                  logic

  Database access         SQLx                    Async PostgreSQL
                                                  queries and
                                                  compile-time checked
                                                  SQL where practical

  Database                PostgreSQL              Transactional and
                                                  relational data

  Migrations              SQLx migrations         Versioned database
                                                  schema

  Authentication          Server-side sessions or User identity
                          short-lived access      
                          tokens with refresh     
                          strategy                

  Reverse proxy           Caddy or Nginx          TLS termination and
                                                  routing

  Deployment              Linux server, systemd,  Production runtime
                          PostgreSQL service      

  Observability           `tracing`, structured   Diagnostics
                          logs, health checks     
  -----------------------------------------------------------------------

These are proposed defaults, not mandatory dependencies. Avoid
introducing Redis, message brokers, Kubernetes, or microservices until
measured requirements justify them.

## 6. Application architecture

Use a **modular monolith**. The API is one deployable Rust application,
but code is separated by business domain. Each module owns its rules and
data access boundaries.

``` mermaid
flowchart TB
    UI[Vue 3 UI]
    HTTP[Axum Routes and Middleware]
    AUTH[Authentication and Authorization]
    APP[Application Use Cases]
    CAT[Product Catalogue]
    SCAN[Product Scanning]
    INV[Inventory]
    PUR[Purchasing]
    POS[Sales and POS]
    CUS[Customers]
    SUP[Suppliers]
    REP[Reporting]
    AUD[Audit Log]
    DB[(PostgreSQL)]

    UI --> HTTP
    HTTP --> AUTH
    AUTH --> APP
    APP --> CAT
    APP --> SCAN
    APP --> INV
    APP --> PUR
    APP --> POS
    APP --> CUS
    APP --> SUP
    APP --> REP
    APP --> AUD
    CAT --> DB
    SCAN --> DB
    INV --> DB
    PUR --> DB
    POS --> DB
    CUS --> DB
    SUP --> DB
    REP --> DB
    AUD --> DB
```

### 6.1 Backend layers

Within each domain, keep a consistent separation:

-   **HTTP/API layer:** extracts path, query, headers, and JSON;
    authenticates and authorizes; maps application results to HTTP
    responses.
-   **Application layer:** coordinates use cases and transaction
    boundaries.
-   **Domain layer:** owns business concepts, invariants, and validation
    that do not depend on HTTP or SQL.
-   **Infrastructure layer:** SQLx repositories, file storage, clocks,
    ID generation, and external integrations.

Do not put SQL queries, stock calculations, and payment rules directly
in Axum handlers.

### 6.2 Transaction boundaries

A use case that changes multiple related records must run in one
database transaction. Examples:

-   Completing a sale: create sale, sale lines, payment records, stock
    movements, and update stock balances.
-   Receiving a purchase: create goods-received records, stock
    movements, and update balances.
-   Returning an item: create return records, determine disposition, and
    optionally add sellable stock.
-   Adjusting stock: create an adjustment and corresponding movement
    after authorization.

If any required step fails, roll back the transaction. Do not return
success if only part of a stock or sales operation committed.

## 7. Domain modules

### 7.1 Dashboard

Provides a summary of current business activity:

-   Today's sales and transaction count.
-   Low-stock and out-of-stock products.
-   Recent purchases and goods receipts.
-   Recent stock adjustments and returns.
-   Sales trends and best-selling products.

Dashboard figures are derived from transactional data. For the initial
release, ordinary SQL aggregate queries are sufficient. Add summary
tables or materialized views only if query performance requires them.

### 7.2 Product catalogue

The catalogue defines what a product is. It must be separate from stock
quantities.

A product should support:

-   Internal product ID.
-   Product name and optional description.
-   Brand and category.
-   Manufacturer/model/product code.
-   Internal SKU, if assigned.
-   Unit of measure (piece, set, box, etc.).
-   Pack quantity where relevant.
-   Dimensions and other structured attributes.
-   Default cost and selling price, if maintained.
-   Tax classification/rate, if applicable.
-   Product image(s), if available.
-   Active/inactive status.
-   Search aliases and alternate codes.

Do not assume every printed code is globally unique. Enforce uniqueness
only within a defined scope and allow an administrator to resolve
duplicate or conflicting supplier codes.

**Important distinction:** - `product_code` is a code printed by the
manufacturer or supplier. - `sku` is the retailer's internal
stock-keeping identifier. - `barcode` is an optional machine-readable
identifier. - `product_id` is the system's stable primary key.

These identifiers are not interchangeable.

### 7.3 Product scanning and OCR

The scanning feature is a client-side assisted data-entry workflow.

#### Workflow

1.  User opens the scanner and grants camera permission.
2.  Browser captures an image or accepts an uploaded image.
3.  User crops or confirms the label region.
4.  Image preprocessing improves orientation, contrast, and readability.
5.  OCR worker detects text and returns recognized lines, confidence
    values, and bounding polygons.
6.  A deterministic parser extracts candidate fields.
7.  The frontend displays candidates in an editable review form.
8.  The backend searches for an existing product using the strongest
    available identifiers.
9.  User selects an existing match or creates a new product.
10. Product creation is saved only after validation and confirmation.

#### OCR responsibilities

OCR should return raw recognition output. It should not decide inventory
quantities, create products, or write to the database.

Example conceptual output:

``` json
{
  "lines": [
    { "text": "1 Pcs Set", "confidence": 0.98 },
    { "text": "Round", "confidence": 0.99 },
    { "text": "Midi 4.75", "confidence": 0.97 },
    { "text": "(H1:3)", "confidence": 0.94 },
    { "text": "W/Plastic Lid", "confidence": 0.98 },
    { "text": "B 6275-4.75", "confidence": 0.96 },
    { "text": "Sugar", "confidence": 0.99 }
  ]
}
```

Values above are illustrative, not measured confidence results.

#### Deterministic parsing

Use a staged parser:

1.  Normalize Unicode punctuation, whitespace, and case while preserving
    the original OCR text.
2.  Detect likely product codes using configurable patterns.
3.  Detect dimensions and units.
4.  Detect quantity and pack/set wording.
5.  Detect known attributes such as shape, material, lid type, colour,
    and product category.
6.  Treat remaining text as description/name candidates.
7.  Return candidates with source line and confidence; never silently
    invent missing values.

Example TypeScript parsing rules:

``` typescript
const patterns = {
  productCode: /\bB\s?\d{4,}(?:-\d+(?:\.\d+)?)?\b/i,
  quantity: /\b(\d+)\s*pcs?\s*set\b/i,
  dimension: /(\d+(?:\.\d+)?)\s*(inches|inch|in|["″])/i,
  height: /\bH1?\s*:\s*(\d+(?:\.\d+)?)/i,
};
```

These are initial rules for observed labels, not universal product-label
grammar. Build a test corpus from real labels and refine patterns
against it.

#### Client-side performance

-   Load the OCR model lazily when the scanner is first opened.
-   Run inference in a Web Worker so the UI remains responsive.
-   Reuse the initialized OCR worker across scans.
-   Cache model assets using browser caching mechanisms where supported.
-   Show model download and scan progress.
-   Handle unsupported browsers and low-memory devices gracefully.
-   Provide manual entry and image upload as fallbacks.
-   Do not upload original images unless the user explicitly chooses a
    server-side workflow or the application needs an image saved as a
    product asset.

OCR model size, inference speed, WebGPU availability, and browser memory
usage must be measured on the actual devices used in the shop. WebGPU
should be an optional acceleration path, not a hard requirement.

#### Product matching order

1.  Exact manufacturer/product code.
2.  Exact internal SKU.
3.  Exact barcode, if present.
4.  Normalized product name plus brand and dimensions.
5.  PostgreSQL trigram/fuzzy text search as suggestions only.

Fuzzy matches must never automatically merge products. Display possible
matches and let a user confirm.

### 7.4 Inventory

Inventory tracks quantities at a location and records why each quantity
changed.

Core capabilities:

-   Stock on hand.
-   Stock reserved (future, if order workflows require it).
-   Available stock.
-   Goods received.
-   Sales deductions.
-   Customer returns.
-   Supplier returns.
-   Damaged/unsellable stock.
-   Manual adjustments.
-   Stock counts and reconciliation.
-   Transfers between locations (future or when multi-location support
    is enabled).

#### Ledger principle

Every stock change creates an immutable stock movement. A balance table
may be maintained for fast reads, but it is a projection of the
movements and must be updated in the same transaction.

Conceptual movement types:

-   `purchase_receipt`
-   `sale`
-   `customer_return`
-   `supplier_return`
-   `adjustment_in`
-   `adjustment_out`
-   `damage`
-   `transfer_in`
-   `transfer_out`
-   `opening_balance`

A movement should include product, location, quantity delta, unit cost
where relevant, source document, actor, timestamp, and reason.

Do not permit arbitrary edits to a product's stock balance. Use an
adjustment workflow that records the before/after difference and reason.

#### Quantity rules

Use a consistent base unit for each product. For products sold as sets
or packs, define whether inventory is counted as individual pieces or
saleable packs. Do not mix units without explicit conversion rules.

For example, a case of 12 pieces can be represented as 12 base units
only if the product definition explicitly establishes that conversion.
Store quantities using an appropriate numeric type and avoid
floating-point arithmetic for stock quantities.

### 7.5 Purchasing and goods receiving

Purchasing records stock acquired from suppliers.

Suggested workflow:

1.  Create purchase order (optional for small operations).
2.  Select supplier and add product lines.
3.  Record ordered quantities and expected costs.
4.  Receive all or part of the order.
5.  Record actual received quantities, cost, and discrepancies.
6.  Confirm receipt.
7.  Create stock movements and update inventory balances.
8.  Record supplier invoice/payment status separately from physical
    receipt.

A purchase order is not stock. Stock increases only when goods are
received and confirmed.

Support partial receipts. A purchase may be received in multiple
deliveries.

### 7.6 Suppliers

Supplier records should include:

-   Name and contact information.
-   Address and tax identifiers where required.
-   Payment terms.
-   Notes.
-   Active status.
-   Purchase history and outstanding amounts (if tracked).

Avoid storing unnecessary personal data. Restrict access to supplier
financial details.

### 7.7 POS and sales

The POS must prioritize speed, clear totals, and protection against
duplicate submissions.

Core workflow:

1.  Cashier opens a sale.
2.  Searches or selects products.
3.  Adds quantities and applies permitted discounts.
4.  System calculates line totals, tax, and grand total using
    server-side rules.
5.  Cashier selects payment method(s).
6.  Cashier confirms payment.
7.  Backend validates stock policy and commits sale, payments, stock
    movements, and balance changes atomically.
8.  System returns a receipt number and printable receipt data.

Support, as needed:

-   Cash.
-   UPI/card/bank payment recorded as tender types.
-   Split tender (one sale paid using multiple methods).
-   Discounts with permission limits.
-   Suspended/held carts.
-   Receipt reprint.
-   Sale cancellation/void with authorization.
-   Customer returns and refunds.

Do not assume that recording a UPI payment proves that money was
received. If a payment gateway is integrated later, verify provider
callbacks/webhooks and use idempotency keys.

#### Sale and stock policy

Define whether the POS may sell below zero stock. Recommended default:
prevent checkout when available stock is insufficient. If the business
explicitly needs negative stock, make it a privileged configuration and
flag resulting balances for reconciliation.

#### Returns

A return must reference the original sale where possible. Returned goods
do not automatically become sellable stock. The operator must choose a
disposition such as sellable, damaged, or inspection required.

### 7.8 Customers

Customer records are optional for walk-in sales. Support:

-   Name and contact details.
-   Purchase history.
-   Notes.
-   Optional credit balance only if a controlled credit-sales feature is
    implemented.

Do not make customer registration mandatory for ordinary cash sales
unless business requirements demand it.

### 7.9 Pricing and taxes

Keep product cost and selling price distinct.

Recommended concepts:

-   Default purchase cost.
-   Current selling price.
-   Historical purchase cost per receipt line.
-   Sale-time unit price snapshot.
-   Discount amount and reason.
-   Tax rate/category snapshot on each sale line.

A sale must preserve the price and tax calculation that applied at the
time of sale. Updating a product's current price must not rewrite
historical sales.

Tax configuration must be reviewed against the business's applicable
Indian tax obligations before production. This architecture does not
prescribe a tax rate or substitute for accounting/tax advice.

### 7.10 Reports

Initial reports:

-   Daily sales summary.
-   Sales by product/category.
-   Gross sales, discounts, tax, and net sales.
-   Purchase summary.
-   Current stock on hand.
-   Low-stock products.
-   Stock movement history.
-   Stock valuation using a clearly defined costing method.
-   Returns and voids.
-   Cashier shift summary, if shifts are implemented.

Every report should state its date range, timezone, and calculation
basis. Do not label revenue as profit. Profit calculations require a
defined cost method and treatment of discounts, returns, and taxes.

### 7.11 Users, roles, and audit

Use permission-based authorization. Example permissions:

-   `products.read`, `products.create`, `products.update`,
    `products.archive`
-   `inventory.read`, `inventory.receive`, `inventory.adjust`,
    `inventory.transfer`
-   `purchases.read`, `purchases.create`, `purchases.receive`
-   `sales.read`, `sales.create`, `sales.void`, `sales.return`
-   `reports.read`
-   `users.manage`, `settings.manage`

High-risk operations such as stock adjustments, sale voids, refunds, and
price overrides should require specific permissions and capture a
reason.

Audit log entries should record actor, action, entity type and ID,
timestamp, request/correlation ID, and relevant before/after values.
Never log passwords, session tokens, or payment secrets.

## 8. Data architecture

PostgreSQL is the source of truth. Use foreign keys, unique constraints,
check constraints, and transactions to enforce invariants as close to
the data as practical.

### 8.1 Core entities

  Entity                   Purpose
  ------------------------ -------------------------------------------------
  `users`                  Staff accounts
  `roles`                  Role definitions
  `permissions`            Permission definitions
  `user_roles`             User-role assignment
  `role_permissions`       Role-permission assignment
  `products`               Canonical product records
  `product_identifiers`    SKU, manufacturer code, barcode, aliases
  `categories`             Product categories
  `brands`                 Brand records
  `product_images`         Image metadata and storage keys
  `locations`              Store/warehouse locations
  `stock_balances`         Current quantity projection by product/location
  `stock_movements`        Immutable stock ledger
  `suppliers`              Supplier records
  `purchase_orders`        Supplier orders
  `purchase_order_items`   Ordered product lines
  `goods_receipts`         Physical receiving documents
  `goods_receipt_items`    Received quantities and costs
  `customers`              Optional customer records
  `sales`                  Sale header
  `sale_items`             Sale lines and price/tax snapshots
  `payments`               Tender/payment records
  `returns`                Return header
  `return_items`           Returned sale lines and disposition
  `audit_logs`             Security and business audit events
  `settings`               Business configuration

### 8.2 Simplified relationship diagram

``` mermaid
erDiagram
    PRODUCTS ||--o{ PRODUCT_IDENTIFIERS : has
    PRODUCTS ||--o{ STOCK_BALANCES : has
    PRODUCTS ||--o{ STOCK_MOVEMENTS : changes
    LOCATIONS ||--o{ STOCK_BALANCES : stores
    LOCATIONS ||--o{ STOCK_MOVEMENTS : records
    SUPPLIERS ||--o{ PURCHASE_ORDERS : receives
    PURCHASE_ORDERS ||--o{ PURCHASE_ORDER_ITEMS : contains
    GOODS_RECEIPTS ||--o{ GOODS_RECEIPT_ITEMS : contains
    PRODUCTS ||--o{ PURCHASE_ORDER_ITEMS : ordered
    PRODUCTS ||--o{ GOODS_RECEIPT_ITEMS : received
    SALES ||--o{ SALE_ITEMS : contains
    SALES ||--o{ PAYMENTS : paid_by
    PRODUCTS ||--o{ SALE_ITEMS : sold
    SALES ||--o{ RETURNS : may_have
    RETURNS ||--o{ RETURN_ITEMS : contains
    PRODUCTS ||--o{ RETURN_ITEMS : returned
```

### 8.3 Important schema decisions

-   Use UUIDs or another stable non-sequential public identifier for
    externally exposed records. Internal database-generated numeric IDs
    are also acceptable if never treated as secrets.
-   Use `NUMERIC`/decimal types for money, not binary floating point.
-   Store currency explicitly (initially likely INR) and do not assume
    every future business uses the same currency.
-   Store timestamps in UTC (`timestamptz`) and display them using the
    business's configured timezone.
-   Use soft deletion/archive status for products referenced by
    historical transactions. Never cascade-delete sales or stock
    movements.
-   Store sale-time product name, SKU, unit price, discount, tax, and
    unit snapshots on sale lines so historical documents remain
    understandable.
-   Store purchase-time cost and supplier description on receipt lines.
-   Use unique constraints for identifiers within their intended scope.
-   Add indexes based on real query patterns and inspect query plans
    before adding many indexes.

### 8.4 Example product model

Illustrative fields, not a final migration:

``` sql
CREATE TABLE products (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    brand_id UUID,
    category_id UUID,
    base_unit TEXT NOT NULL DEFAULT 'piece',
    pack_quantity NUMERIC(14, 3),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE product_identifiers (
    id UUID PRIMARY KEY,
    product_id UUID NOT NULL REFERENCES products(id),
    identifier_type TEXT NOT NULL,
    identifier_value TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (identifier_type, normalized_value)
);
```

The uniqueness constraint above assumes identifiers are globally unique
within an identifier type. If the supplier's codes are only unique per
brand or supplier, include that scope in the schema and constraint
instead.

### 8.5 Stock balance and ledger consistency

`stock_balances` is a read-optimised projection. `stock_movements` is
the audit source.

For every committed movement: 1. Lock the relevant balance row (or
safely create/lock it). 2. Validate the operation against stock policy.
3. Insert the movement. 4. Update the balance. 5. Commit both together.

A reconciliation job should periodically compare the balance projection
with the sum of ledger movements. Any mismatch should be reported for
investigation, not silently corrected.

## 9. API design

Use versioned JSON REST endpoints under `/api/v1`. Return consistent
error objects and validate all input on the server.

### 9.1 Endpoint outline

  ----------------------------------------------------------------------
  Domain                             Example endpoints
  ---------------------------------- -----------------------------------
  Authentication                     `POST /auth/login`,
                                     `POST /auth/logout`, `GET /auth/me`

  Products                           `GET /products`, `POST /products`,
                                     `GET /products/{id}`,
                                     `PATCH /products/{id}`

  Product lookup                     `GET /products/lookup?code=...`,
                                     `GET /products/search?q=...`

  Identifiers                        `POST /products/{id}/identifiers`

  Inventory                          `GET /inventory/balances`,
                                     `GET /inventory/movements`

  Adjustments                        `POST /inventory/adjustments`

  Purchases                          `GET /purchases`,
                                     `POST /purchases`,
                                     `POST /purchases/{id}/receive`

  Suppliers                          `GET /suppliers`, `POST /suppliers`

  POS                                `POST /sales`, `GET /sales/{id}`,
                                     `POST /sales/{id}/void`

  Returns                            `POST /returns`

  Reports                            `GET /reports/sales-summary`,
                                     `GET /reports/stock`

  Users                              `GET /users`, `POST /users`,
                                     `PATCH /users/{id}`
  ----------------------------------------------------------------------

OCR is client-side, so there is no mandatory `/ocr` endpoint. The
browser sends the confirmed structured product data to the normal
product API. If a future server-side OCR option is added, expose it
separately and keep it optional.

### 9.2 Example product creation request

``` json
{
  "name": "Sugar",
  "brand_name": "Ektra",
  "description": "Round midi container with plastic lid",
  "manufacturer_code": "B6275-4.75",
  "attributes": {
    "shape": "Round",
    "size": "Midi",
    "diameter_inches": 4.75,
    "height_inches": 3,
    "lid_material": "Plastic"
  },
  "pack_quantity": 1,
  "base_unit": "piece"
}
```

The backend must validate, normalize, and persist this request. It
should not trust client-provided permissions, prices, stock balances,
tax totals, or calculated sale totals.

### 9.3 Response and error conventions

Use appropriate HTTP status codes:

-   `200` successful read/update.
-   `201` resource created.
-   `204` successful operation with no response body.
-   `400` malformed request.
-   `401` unauthenticated.
-   `403` authenticated but not authorized.
-   `404` resource not found.
-   `409` conflict, duplicate identifier, or invalid state transition.
-   `422` well-formed input that fails domain validation.
-   `500` unexpected server error.

Example:

``` json
{
  "error": {
    "code": "PRODUCT_IDENTIFIER_CONFLICT",
    "message": "This manufacturer code is already assigned to another product.",
    "request_id": "..."
  }
}
```

Do not expose SQL errors, stack traces, internal file paths, or secrets
in API responses.

### 9.4 Idempotency

Sales, payments, goods receipts, and other operations that may be
retried need idempotency protection. A client-generated idempotency key
should be unique per operation and stored with the resulting
transaction. Repeating the same request with the same key must not
create a second sale or receipt.

## 10. Frontend architecture

### 10.1 Application areas

Suggested navigation:

-   Overview
-   POS
-   Products
-   Scan Product
-   Inventory
-   Purchases
-   Suppliers
-   Customers
-   Reports
-   Users and Settings

Show navigation based on permissions. Hiding a button is not
authorization; the API must enforce permissions independently.

### 10.2 Suggested Vue structure

``` text
frontend/
├── public/
├── src/
│   ├── app/
│   │   ├── router/
│   │   ├── providers/
│   │   └── layouts/
│   ├── modules/
│   │   ├── auth/
│   │   ├── dashboard/
│   │   ├── products/
│   │   ├── scanner/
│   │   │   ├── components/
│   │   │   ├── composables/
│   │   │   ├── workers/
│   │   │   ├── parsers/
│   │   │   └── types/
│   │   ├── inventory/
│   │   ├── purchases/
│   │   ├── suppliers/
│   │   ├── pos/
│   │   ├── customers/
│   │   ├── reports/
│   │   └── administration/
│   ├── shared/
│   │   ├── api/
│   │   ├── components/
│   │   ├── composables/
│   │   ├── types/
│   │   ├── validation/
│   │   └── utils/
│   ├── assets/
│   └── main.ts
├── package.json
└── vite.config.ts
```

Organize by business feature rather than putting every component in
global `components/` or every API call in one large file.

### 10.3 State management

-   Use component state for temporary UI state.
-   Use composables for reusable camera, OCR, and form logic.
-   Use Pinia for genuinely shared state such as authenticated user,
    business configuration, and an active POS cart if it spans multiple
    routes/components.
-   Treat server data as server-owned. Refresh or invalidate it after
    mutations rather than maintaining multiple competing copies of
    inventory balances.
-   Persist only appropriate non-sensitive drafts locally. Do not store
    passwords or long-lived secrets in local storage.

### 10.4 Scanner user experience

The scanner should display: - Camera preview and capture controls. - A
crop/retake step. - OCR progress and model-loading status. - Recognized
text with confidence indicators. - Editable fields for product code,
name, dimensions, and pack quantity. - Existing catalogue matches. - A
clear distinction between "create product" and "add stock". - A
confirmation step before any database mutation.

If OCR fails, the user must still be able to type the product details
manually.

## 11. Rust backend project structure

A suggested structure for a modular monolith:

``` text
backend/
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── migrations/
│   ├── 0001_create_users.sql
│   ├── 0002_create_catalogue.sql
│   ├── 0003_create_inventory.sql
│   ├── 0004_create_purchasing.sql
│   └── 0005_create_sales.sql
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── state.rs
│   ├── error.rs
│   ├── http/
│   │   ├── mod.rs
│   │   ├── routes.rs
│   │   ├── middleware/
│   │   └── extractors/
│   ├── auth/
│   ├── catalog/
│   │   ├── mod.rs
│   │   ├── routes.rs
│   │   ├── service.rs
│   │   ├── repository.rs
│   │   ├── models.rs
│   │   └── dto.rs
│   ├── inventory/
│   ├── purchasing/
│   ├── sales/
│   ├── suppliers/
│   ├── customers/
│   ├── reporting/
│   ├── audit/
│   └── shared/
│       ├── database/
│       ├── ids/
│       └── validation/
└── tests/
    ├── api/
    └── integration/
```

Each module can follow the same internal pattern:

-   `routes.rs`: HTTP routes and request/response mapping.
-   `dto.rs`: API input/output types.
-   `service.rs`: use cases and transaction orchestration.
-   `repository.rs`: database queries.
-   `models.rs`: persistence/domain data structures.
-   `mod.rs`: module exports.

Do not create a generic abstraction for every table before there is a
real need. SQLx queries and explicit repositories are appropriate for a
system with important transactional rules.

### 11.1 Runtime state

Create shared application state once at startup. It may contain a
PostgreSQL connection pool, configuration, and shared service
dependencies. Prefer `PgPool` over putting one `tokio_postgres::Client`
behind every handler; a pool is designed to serve concurrent requests.

Use async operations for database and network I/O. Use `tokio::spawn`
only for independent background work that has a clear lifecycle and
error-handling strategy. Do not spawn a task for every ordinary database
query.

## 12. Security architecture

### 12.1 Authentication

Use established password hashing such as Argon2id. Never store plaintext
passwords or reversible password encryption.

For a browser application, choose one coherent session strategy: -
Secure, HttpOnly, SameSite cookies with server-side session management;
or - A carefully designed short-lived access-token and refresh-token
flow.

For a same-origin web application, secure cookie-based sessions are
often simpler. Apply CSRF protection to state-changing requests when
cookie authentication is used.

### 12.2 Authorization

Enforce authorization on every protected backend operation. Use least
privilege. A cashier should not automatically be allowed to edit product
costs, adjust stock, void arbitrary sales, or manage users.

### 12.3 General controls

-   HTTPS everywhere in production.
-   Rate-limit login and other abuse-prone endpoints.
-   Validate and bound request sizes.
-   Use parameterized SQL; never concatenate user input into SQL.
-   Restrict CORS to known frontend origins.
-   Apply secure cookie settings.
-   Keep secrets in environment/configuration management, not source
    control.
-   Rotate credentials and document recovery procedures.
-   Audit privileged actions.
-   Back up database and uploaded files.
-   Test restore procedures.
-   Keep dependencies updated and review security advisories.

### 12.4 Image and file handling

If product images are stored: - Validate actual file type, not only
extension. - Enforce size limits. - Generate server-side storage
names. - Do not execute uploaded files. - Store outside the public
application directory or in object storage. - Serve with safe content
headers. - Remove EXIF metadata if it is not required.

## 13. Reliability and operational behaviour

### 13.1 Database reliability

-   Use PostgreSQL transactions for multi-record business operations.
-   Use constraints to prevent invalid references and duplicate
    identifiers.
-   Configure connection pool limits based on database capacity.
-   Set query and request timeouts.
-   Use migrations for all schema changes.
-   Test migrations against a copy of realistic data before production.

### 13.2 Background jobs

The initial application may not need a separate queue. Use synchronous
transactions for critical stock and sales operations.

Introduce a background worker only for tasks such as: - Generating large
exports. - Sending notifications. - Scheduled low-stock summaries. -
Image processing that is too expensive for request handling. - Periodic
stock-ledger reconciliation.

Background tasks must be retryable and idempotent. A failed notification
must not undo a completed sale.

### 13.3 Backups

At minimum: - Automated daily database backups. - More frequent WAL
archiving or point-in-time recovery if the business cannot tolerate
losing a full day's transactions. - Off-site encrypted backup storage. -
Retention policy. - Periodic restore tests. - Backup of product images
and other uploaded assets.

A backup is not proven until it has been restored successfully.

### 13.4 Observability

Use structured logs with `tracing`. Include request IDs and useful
business identifiers, but never log credentials, tokens, full payment
details, or unnecessary customer data.

Expose: - Liveness endpoint: process is running. - Readiness endpoint:
required dependencies are available. - Application version/build
information. - Database connection health. - Error rates and request
latency through monitoring/logging infrastructure.

## 14. Deployment architecture

For the first production deployment, use one Linux server or a small
number of clearly separated services. Keep PostgreSQL data outside the
application container/process lifecycle.

``` mermaid
flowchart TB
    Client[Browser / POS Device]
    DNS[DNS and optional CDN]
    Proxy[Caddy or Nginx - HTTPS]
    Web[Static Vue Build]
    API[Rust Axum Service]
    DB[(PostgreSQL)]
    Store[Product Image Storage]
    Backup[Off-site Backup Target]

    Client --> DNS
    DNS --> Proxy
    Proxy --> Web
    Proxy --> API
    API --> DB
    API --> Store
    DB --> Backup
    Store --> Backup
```

### 14.1 Production processes

-   Reverse proxy serves the Vue build and forwards `/api` to the Rust
    API.
-   Rust API runs as a dedicated unprivileged systemd service.
-   PostgreSQL runs as a managed service or dedicated database host.
-   Product images are stored on persistent storage or object storage.
-   TLS certificates renew automatically.
-   Logs are collected and rotated.
-   Firewall exposes only required ports.

Docker is optional. If used, persist PostgreSQL data in a managed volume
or external database. Never rely on a container's writable layer for
database persistence.

### 14.2 Environment configuration

Example backend `.env.example`:

``` dotenv
APP_ENV=development
APP_HOST=127.0.0.1
APP_PORT=8000
APP_TIMEZONE=Asia/Kolkata

DATABASE_URL=postgres://user:password@localhost:5432/jehovah_jire
DATABASE_MAX_CONNECTIONS=10

SESSION_SECRET=replace-with-a-long-random-secret
FRONTEND_ORIGIN=http://localhost:5173

STORAGE_PATH=./storage
```

This is an example only. Production secrets must be generated securely
and injected through the deployment environment. Do not commit a real
`.env` file.

## 15. Testing strategy

### 15.1 Unit tests

Test pure domain logic: - Product code normalization. - OCR text
parsing. - Dimension and quantity parsing. - Price and tax
calculations. - Discount limits. - Stock validation. - Return
disposition rules. - Permission checks.

Build a labelled OCR test corpus containing real product-label images
and expected extracted fields. Include blurry, rotated, low-light,
partially obscured, and unusual labels.

### 15.2 Integration tests

Use a real disposable PostgreSQL database or isolated test database to
test: - Product creation and identifier uniqueness. - Purchase receiving
and stock ledger updates. - Sale creation and stock deduction. -
Concurrent sales of the last available unit. - Returns and stock
disposition. - Stock adjustment authorization. - Transaction rollback on
failure. - Idempotent retries. - Migration up/down strategy where
supported.

### 15.3 End-to-end tests

Test complete workflows: - Log in as a cashier. - Scan or manually
create a product. - Confirm product details. - Receive stock. - Sell the
product. - Print/retrieve receipt. - Return an item. - Verify stock
movement and report results.

### 15.4 Concurrency tests

Inventory is particularly sensitive to concurrent operations. Two
cashiers must not both sell the last unit if negative stock is
prohibited. Use row-level locking or another correct concurrency
strategy inside a transaction. Test the behaviour rather than assuming
it is safe.

## 16. Performance considerations

-   Paginate product, sale, purchase, and movement lists.
-   Add indexes for product code, SKU, normalized identifiers,
    timestamps, and common foreign keys.
-   Avoid loading the entire product catalogue into the browser.
-   Debounce search input and cancel stale requests.
-   Keep OCR inference off the main UI thread.
-   Lazy-load OCR model assets.
-   Compress and resize product images.
-   Use database connection pooling.
-   Inspect slow queries before adding caching.
-   Prefer correctness over a cache for stock balances and payment
    state.

A product search endpoint should support exact identifier lookup and
text search separately. Exact code lookup should be fast and
deterministic.

## 17. Important business rules

These rules should be written into tests and enforced by the backend:

1.  OCR output never creates a product or stock movement without user
    confirmation.
2.  Creating a product does not add stock.
3.  A purchase order does not add stock; confirmed goods receipt does.
4.  A completed sale reduces stock exactly once.
5.  A retried sale request cannot create duplicate sales.
6.  A return does not automatically make an item sellable.
7.  Historical sale prices and tax values do not change when catalogue
    prices change.
8.  Stock adjustments require permission and a reason.
9.  Completed financial and stock transactions are not hard-deleted.
10. Product identifiers must be unique within their defined scope.
11. The backend calculates totals; client-provided totals are not
    trusted.
12. Stock balance projections must reconcile with the stock movement
    ledger.

## 18. Implementation roadmap

Build vertical slices that deliver working business value. Do not build
every database table and screen before testing an end-to-end workflow.

### Phase 1 --- Foundation

-   Create Rust Axum project.
-   Configure PostgreSQL and SQLx.
-   Add migrations and configuration.
-   Create Vue 3 + TypeScript application.
-   Implement authentication and permission middleware.
-   Add structured logging and health checks.
-   Establish API error conventions and automated tests.

**Exit criteria:** a user can log in, access protected routes, and the
frontend can communicate with the API.

### Phase 2 --- Product catalogue

-   Create brands and categories.
-   Create products and identifiers.
-   Implement product create/edit/archive.
-   Implement exact identifier and text search.
-   Add product detail and list screens.
-   Establish product unit and pack-quantity rules.

**Exit criteria:** staff can maintain a reliable product catalogue
manually.

### Phase 3 --- Browser scanner

-   Implement camera capture and image upload.
-   Integrate PaddleOCR.js in a Web Worker.
-   Add crop/rotate and basic image preprocessing.
-   Implement deterministic parser with tests.
-   Add review/edit screen.
-   Match scanned product codes against the catalogue.
-   Add manual fallback.

**Exit criteria:** a staff member can scan a representative set of real
Ektra labels, correct the extracted data, and create or find a product.
Measure field-level accuracy on a labelled test set.

### Phase 4 --- Inventory and purchasing

-   Add locations.
-   Add stock balances and immutable movements.
-   Implement adjustments with permissions and reasons.
-   Add suppliers.
-   Add purchase orders if required.
-   Implement goods receipt and partial receipt.
-   Add low-stock and movement screens.
-   Add reconciliation tests.

**Exit criteria:** received stock can be traced from supplier receipt to
current stock balance.

### Phase 5 --- POS and sales

-   Build fast product search and cart.
-   Add quantity, price, discount, and tax rules.
-   Add payment records and split tender if required.
-   Implement atomic sale completion and stock deduction.
-   Add receipt generation.
-   Add void and return workflows with permissions.
-   Add idempotency protection.

**Exit criteria:** a cashier can complete a sale and the inventory
ledger remains correct under retries and concurrent checkout.

### Phase 6 --- Reports and operational readiness

-   Add sales, purchase, stock, and movement reports.
-   Add audit log views.
-   Add backup automation and restore testing.
-   Add monitoring and alerting.
-   Perform security review.
-   Test on actual shop devices and network conditions.
-   Document staff procedures.

**Exit criteria:** the system can be operated and recovered reliably in
the intended retail environment.

### Phase 7 --- Future expansion

Only after the core system is stable, consider: - Multiple stores and
stock transfers. - Multi-business tenancy. - Offline POS with a
carefully designed sync/conflict model. - Supplier imports and catalogue
synchronisation. - Customer loyalty. - Payment gateway integration. -
Accounting exports. - Advanced stock valuation and reorder suggestions.

## 19. Decisions to confirm before implementation

The following are business decisions, not details the framework should
guess:

  ---------------------------------------------------------------------
  Decision                           Recommended starting point
  ---------------------------------- ----------------------------------
  Number of businesses               One business initially

  Store locations                    One location, but model locations
                                     explicitly

  Currency                           INR, configurable

  Timezone                           Asia/Kolkata, configurable

  Product identifier                 Manufacturer code plus internal
                                     SKU

  Barcode                            Optional

  OCR                                Browser-side PaddleOCR.js

  OCR fallback                       Manual entry

  Stock unit                         Explicit base unit per product

  Negative stock                     Disabled

  Product creation                   Requires human confirmation

  Stock changes                      Ledger-backed transactions

  POS payments                       Record tender types; integrate
                                     gateways only when needed

  Offline operation                  Not in initial release unless
                                     required by the shop

  Deployment                         Linux + reverse proxy + Rust
                                     service + PostgreSQL
  ---------------------------------------------------------------------

## 20. Definition of a successful first release

The first useful release does not need every feature in this document.
It should reliably support this complete workflow:

1.  An administrator creates staff accounts and product categories.
2.  A staff member scans an Ektra product label.
3.  The browser recognizes printed text locally.
4.  The parser extracts a product code and candidate attributes.
5.  The staff member reviews the result.
6.  The system finds an existing product or creates one.
7.  The staff member receives a quantity into inventory.
8.  A cashier searches for and sells that product.
9.  The system records payment and reduces stock exactly once.
10. A manager can inspect the sale, receipt, stock movements, and
    current balance.

If this workflow is correct, auditable, and usable on the shop's actual
devices, the core architecture is delivering value.

------------------------------------------------------------------------

## Appendix A --- Naming conventions

-   Product: **Jehovah Jireh**
-   Short name: **JJ Retail**
-   Full product description: **Jehovah Jireh Retail Management System**
-   Backend crate: `jehovah-jireh-api` or `jj-retail-api`
-   Frontend package: `jj-retail-web`
-   Database: `jehovah_jireh`
-   API prefix: `/api/v1`

Use `Jehovah Jireh` as the product's display name. Keep internal
identifiers short and consistent.

## Appendix B --- Glossary

  ---------------------------------------------------------------------
  Term                               Meaning
  ---------------------------------- ----------------------------------
  POS                                Point of Sale; the workflow used
                                     to sell products and accept
                                     payment

  IMS                                Inventory Management System;
                                     tracks products and stock

  RMS                                Retail Management System; combines
                                     inventory, sales, purchasing, and
                                     retail operations

  SKU                                Retailer's internal Stock Keeping
                                     Unit

  Manufacturer code                  Product identifier printed or
                                     assigned by the
                                     manufacturer/supplier

  OCR                                Optical Character Recognition;
                                     converts image text into
                                     machine-readable text

  Stock movement                     An auditable record of a quantity
                                     entering, leaving, or changing
                                     status

  Stock balance                      Current quantity projection for a
                                     product at a location

  Goods receipt                      Record confirming that ordered
                                     goods physically arrived

  Idempotency                        Guarantee that retrying the same
                                     operation does not perform it
                                     twice

  Modular monolith                   One deployable application
                                     organized into independent
                                     business modules
  ---------------------------------------------------------------------

## 21. Delivery management and driver workflow

Delivery is a first-class module in Jehovah Jireh. A delivery driver is
a restricted operational user who can view and update deliveries
assigned to them, without gaining access to the full POS, product
catalogue, purchasing, or reporting modules.

### 21.1 Delivery workflow

A sale/order becomes a delivery only when delivery is requested. The
cashier or manager records the delivery address, contact details,
delivery instructions, and delivery fee before confirming the order.

Suggested lifecycle:

``` mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> ReadyForDispatch: Items packed and delivery requested
    ReadyForDispatch --> Assigned: Manager assigns driver
    Assigned --> PickedUp: Driver confirms collection
    PickedUp --> OutForDelivery: Driver starts route
    OutForDelivery --> Delivered: Customer receives order
    OutForDelivery --> Failed: Delivery attempt unsuccessful
    Failed --> Assigned: Reassign or retry
    Failed --> Returned: Items returned to store
    Delivered --> [*]
    Returned --> [*]
```

The exact statuses may be simplified for the first release, but the
system must distinguish an order being prepared from an order physically
handed to a driver and from an order successfully delivered.

Recommended initial statuses: - `pending` - `ready_for_dispatch` -
`assigned` - `picked_up` - `out_for_delivery` - `delivered` -
`delivery_failed` - `returned_to_store` - `cancelled`

Do not let a driver mark an order as delivered before it has been
assigned to them and picked up. Status transitions must be validated by
the backend.

### 21.2 Driver role and permissions

Use a `delivery_driver` role composed of explicit permissions, rather
than special-casing a role name throughout the application.

Example permissions: - `deliveries.read_assigned` -
`deliveries.accept_assigned` (only if drivers may accept jobs) -
`deliveries.update_status` - `deliveries.record_attempt` -
`deliveries.record_proof` - `deliveries.call_customer` (frontend action;
phone number exposure remains restricted)

A driver should: - See only active deliveries assigned to their own user
account. - See the delivery reference, recipient name, delivery address,
contact number, instructions, item summary, and amount due if collection
is required. - Open the address in an available map/navigation
application. - Mark pickup, out-for-delivery, delivered, or failed
according to allowed transitions. - Record a failed attempt reason. -
Capture delivery proof where the business requires it.

A driver should not: - Browse all sales or all customers. - Change
product prices, order lines, discounts, or delivery fees. - Edit stock
balances. - View purchase costs, profit reports, or unrelated customer
history. - Reassign deliveries unless explicitly granted that
permission.

The backend must filter records by the authenticated driver's ID.
Filtering only in the frontend is not access control.

### 21.3 Delivery proof and failed attempts

Depending on the business's needs, proof of delivery can include: -
Customer OTP. - Recipient name and confirmation. - Signature. -
Photograph, only where appropriate and with a clear retention/privacy
policy. - Timestamp and the driver who completed the delivery.

For a first release, a delivered timestamp plus recipient name or OTP is
usually simpler than collecting photographs. Never require a photograph
of a person or their home by default.

A failed attempt should record a reason, such as: - Customer
unavailable. - Address not found. - Customer refused delivery. - Unable
to contact customer. - Other, with a note.

A failed attempt must not automatically mark the sale as refunded or
restore stock. The returned goods must be checked in and their
disposition recorded.

### 21.4 Suggested delivery data model

Add these entities:

  ---------------------------------------------------------------------
  Entity                             Purpose
  ---------------------------------- ----------------------------------
  `delivery_addresses`               Saved customer delivery addresses,
                                     if customer accounts are used

  `deliveries`                       Delivery job linked to a
                                     sale/order

  `delivery_assignments`             Driver assignment history

  `delivery_status_events`           Immutable status history and
                                     timestamps

  `delivery_attempts`                Failed or successful attempt
                                     details

  `delivery_proofs`                  OTP/recipient/signature/photo
                                     metadata, if collected

  `delivery_fee_rules`               Configurable delivery pricing
                                     rules or zones
  ---------------------------------------------------------------------

A delivery record should snapshot the address and recipient details used
for that order. Do not rely solely on a customer's current saved
address, because it may change after an order is placed.

Example conceptual fields for `deliveries`: - `id` - `sale_id` (or order
ID if orders are separated from completed sales) - `status` -
`recipient_name` - `recipient_phone` - `address_line` - `locality` -
`delivery_instructions` - `distance_km` (optional estimate, with
source/method) - `delivery_fee` - `amount_to_collect` (if cash
collection is supported) - `assigned_driver_id` - `created_at`,
`assigned_at`, `picked_up_at`, `delivered_at`

Keep delivery assignment history in its own table so reassignment does
not erase who previously handled the delivery.

## 22. Delivery fee policy

### 22.1 Recommendation: start with locality/zone-based fixed fees

For a typical delivery fee in the ₹50--₹100 range, do not begin with
fully automatic per-kilometre pricing. Start with configurable delivery
zones or locality-based flat rates, and allow a manager to override the
fee with a reason.

Distance-based pricing sounds precise, but it introduces practical
issues: - Straight-line distance is not the same as road distance. - A
route may be longer because of one-way roads, hills, restricted roads,
or access conditions. - A mapping/routing API may require connectivity
and incur usage costs. - The delivery price can appear unpredictable to
customers if it changes with every address. - The driver may have to
make multiple deliveries in one trip, so charging each order the full
route distance can overcharge customers.

For a hilly city or a place with irregular road access, locality-based
pricing is often easier to explain and operate than straight-line
distance.

### 22.2 Example starting tariff

The following is an example policy only. The business must set the
actual localities and fees after checking its delivery costs.

  Delivery zone               Example fee
  ---------------------- ----------------
  Nearby/local zone                   ₹50
  Standard zone                       ₹70
  Extended zone                      ₹100
  Outside service area     Quote manually

The names should be replaced with actual localities or clearly defined
service areas. Do not infer a fee from a customer's address without a
configured rule.

Alternative if the shop prefers distance bands:

  Estimated road distance      Example fee
  ------------------------- --------------
  Up to 2 km                           ₹50
  Over 2 km to 5 km                    ₹70
  Over 5 km to 8 km                   ₹100
  Beyond 8 km                 Manual quote

These are illustrative bands, not a recommendation that these exact
thresholds or prices will cover the business's costs.

### 22.3 Hybrid pricing

A useful later model is:

`delivery_fee = base_fee + distance_band_surcharge + optional_special_handling_fee`

Keep the first release simpler: one configured zone fee, a free-delivery
threshold if the business wants one, and a manager override. Add
route-distance calculation only after collecting real delivery data.

If distance is used: - Use road-route distance, not straight-line
distance. - Store the distance estimate and its source. - Calculate the
fee at checkout and show it before order confirmation. - Snapshot the
fee on the order/delivery so later tariff changes do not rewrite old
orders. - Provide a manual quote/override for inaccessible or unusual
destinations. - Do not continuously recalculate the fee after the
customer has confirmed the order.

### 22.4 Pricing configuration

Managers should be able to configure: - Service zones/localities. - Fee
per zone. - Optional minimum order amount for delivery. - Optional
free-delivery threshold. - Whether out-of-zone addresses require a
manual quote. - Effective date for new fee rules. - Whether a manager
override is permitted and which permission is required.

Store the applied fee and a reference/snapshot of the pricing rule on
the order. Historical deliveries must retain the fee originally quoted.

A manager override should record: - Original calculated fee. - Final
fee. - User who changed it. - Timestamp. - Reason.

### 22.5 Delivery fee accounting

Keep delivery charges separate from product line prices and discounts.
The order should expose a clear breakdown: - Items subtotal. - Product
discounts. - Tax, as applicable. - Delivery fee. - Grand total. - Amount
paid. - Amount due on delivery, if any.

Decide whether delivery fees are taxable and how they should be
represented with the business's accountant/tax adviser. Do not hard-code
tax treatment without confirming applicable rules.

If the driver collects cash, record it as a collection against the
existing sale/payment due; do not create a second sale. Reconcile driver
collections against completed deliveries and cash handovers.

## 23. Delivery API outline

Suggested endpoints:

  ------------------------------------------------------------------------
  Purpose                    Endpoint
  -------------------------- ---------------------------------------------
  Create delivery from an    `POST /deliveries`
  eligible order             

  List deliveries for        `GET /deliveries?status=ready_for_dispatch`
  dispatch staff             

  Assign/reassign driver     `POST /deliveries/{id}/assign`

  Driver's own active        `GET /me/deliveries`
  deliveries                 

  Delivery details           `GET /deliveries/{id}`

  Update delivery status     `POST /deliveries/{id}/status`

  Record attempt             `POST /deliveries/{id}/attempts`

  Record proof of delivery   `POST /deliveries/{id}/proof`

  Configure delivery zones   `GET/POST /delivery-zones`

  Update zone tariff         `PATCH /delivery-zones/{id}`

  Quote delivery fee         `POST /delivery-fees/quote`
  ------------------------------------------------------------------------

`GET /me/deliveries` must return only deliveries assigned to the
authenticated driver. Every status update must validate ownership,
current status, and allowed transition on the server.

## 24. Delivery screens

### Dispatch/manager screens

-   Ready for dispatch queue.
-   Delivery details and address.
-   Assign or reassign driver.
-   Driver workload/active delivery list.
-   Delivery status history.
-   Failed deliveries requiring action.
-   Delivery fee zone configuration.

### Driver screens

Keep the driver interface mobile-first and intentionally simple: - **My
Deliveries:** assigned active jobs. - **Delivery Detail:** customer,
address, phone, items, instructions, and amount to collect. -
**Navigate:** open the address in a map application. - **Update
Status:** picked up, out for delivery, delivered, or failed. -
**Proof/Attempt:** enter OTP/recipient or failure reason. -
**Completed:** recent deliveries assigned to that driver.

Do not expose the full administrative navigation to a driver.

## 25. Delivery implementation roadmap

Add delivery after the core product, inventory, and order/sales
foundations are stable:

1.  Add delivery zones and configurable flat fees.
2.  Add delivery records linked to eligible orders.
3.  Add `delivery_driver` role and permission checks.
4.  Build dispatch queue and driver assignment.
5.  Build the mobile-friendly driver delivery list and detail screens.
6.  Add status transitions and delivery event history.
7.  Add failed-attempt handling and proof of delivery.
8.  Add delivery fee reporting and cash collection reconciliation if
    drivers collect payments.
9.  Evaluate distance-based pricing only after collecting actual
    delivery distance, time, and cost data.

**Delivery release acceptance criteria:** a manager can quote a fee,
confirm a delivery, assign a driver, and see its status; the driver can
see only their own assigned deliveries and complete or fail an attempt;
the order retains its original delivery fee and complete status history.

## 26. Dynamic payment collection and driver settlement

Delivery payment handling must be configurable per order. Do not assume
that the driver always collects from the customer or that the shop
always pays the driver. These are separate business decisions and should
be represented explicitly.

### 26.1 Separate the two money flows

There are two independent questions for every delivery:

1.  **Who collects the order payment from the customer?**
    -   `driver`
    -   `shop`
2.  **Who pays the driver for the delivery service?**
    -   `customer_direct` --- customer pays the delivery fee directly to
        the driver.
    -   `shop_pays_driver` --- shop collects the delivery fee from the
        customer and later settles the driver's delivery compensation.
    -   `included_or_waived` --- delivery is free to the customer, but
        the shop may still owe the driver compensation.

Do not model these as one boolean such as `cash_on_delivery`. Cash on
delivery describes a collection situation, not who is entitled to the
delivery fee or how the driver is compensated.

The most common workflow may be: - Driver collects the order amount and
delivery fee from the customer. - Driver retains the agreed delivery
fee. - Driver remits the merchandise/order amount to the shop.

The occasional alternative may be: - Customer pays the full order total,
including delivery fee, to the shop before dispatch. - Shop pays the
driver the agreed delivery compensation separately.

The system must support both, and must not count the same delivery fee
as both driver income and shop income.

### 26.2 Per-delivery payment configuration

At order confirmation, record a payment arrangement snapshot:

  ----------------------------------------------------------------------------
  Field                          Example values          Meaning
  ------------------------------ ----------------------- ---------------------
  `customer_payment_collector`   `driver`, `shop`        Who receives the
                                                         customer's payment

  `customer_payment_method`      `cash`, `upi`, `card`,  How the customer pays
                                 `bank_transfer`,        
                                 `other`                 

  `delivery_fee_payer`           `customer`, `shop`,     Who bears the
                                 `waived`                delivery charge

  `driver_compensation_type`     `per_delivery`,         How driver
                                 `fixed`, `none`         compensation is
                                                         determined

  `driver_compensation_amount`   `50.00`                 Amount payable to
                                                         driver for this job,
                                                         if applicable

  `delivery_fee_amount`          `70.00`                 Delivery charge
                                                         quoted to customer

  `amount_due_from_customer`     `...`                   Outstanding amount at
                                                         dispatch/checkout

  `amount_collected_by_driver`   `...`                   Amount driver
                                                         actually received

  `amount_remitted_to_shop`      `...`                   Amount driver handed
                                                         over to shop

  `driver_settlement_status`     `not_due`, `pending`,   Status of
                                 `partially_settled`,    shop-to-driver
                                 `settled`               compensation

  `driver_remittance_status`     `not_due`, `pending`,   Status of
                                 `partially_remitted`,   driver-to-shop
                                 `remitted`              collections
  ----------------------------------------------------------------------------

Store monetary values as decimal/numeric amounts and snapshot them on
the order/delivery. Later changes to default policy must not alter
completed or in-progress deliveries.

### 26.3 Example A --- Driver collects from customer

Assume: - Merchandise total: ₹1,000 - Delivery fee: ₹70 - Customer pays
driver: ₹1,070 - Driver keeps delivery compensation: ₹70 - Driver remits
to shop: ₹1,000

The system records: - Customer payment collected: ₹1,070. - Driver
collection: ₹1,070. - Driver delivery compensation earned: ₹70. - Driver
remittance due to shop: ₹1,000. - Driver remittance received by shop:
₹1,000 once handed over.

The driver must not be marked fully settled merely because the delivery
status is `delivered`. The cash remittance is a separate financial
event.

If the driver is not allowed to retain the fee and must remit all
collections, record the full ₹1,070 as due to the shop and record the
driver's compensation separately as payable by the shop. This should be
a configurable policy, not an assumption embedded in code.

### 26.4 Example B --- Shop collects; shop pays driver

Assume: - Merchandise total: ₹1,000 - Delivery fee charged to customer:
₹70 - Customer has paid the shop: ₹1,070 - Driver compensation agreed:
₹70

The system records: - Customer payment received by shop: ₹1,070. -
Customer amount due: ₹0. - Driver compensation payable by shop: ₹70. -
Driver settlement: pending until paid. - Driver-to-shop remittance: not
applicable.

When the shop pays the driver, create a settlement payment record and
mark the compensation as settled. The delivery fee collected from the
customer and the compensation paid to the driver should be separate
ledger entries. They may happen to be the same amount, but they are not
the same transaction.

### 26.5 Example C --- Delivery fee waived

Assume: - Merchandise total: ₹1,000 - Delivery fee charged to customer:
₹0 - Shop agrees to pay driver ₹70

The system records: - Customer pays ₹1,000. - Delivery fee is waived. -
Driver compensation payable by shop: ₹70. - Shop records the ₹70 as a
delivery expense/payable.

This avoids treating a free delivery as unpaid work or incorrectly
assuming that no driver compensation is due.

### 26.6 Driver cash and settlement screen

The driver app should show, for each delivery: - Total amount to collect
from customer. - Payment method expected. - Whether the driver keeps a
delivery fee or must remit the full collection. - Amount to remit to the
shop. - Any driver compensation payable by the shop. - Collection
confirmation and remittance status.

At the end of a shift or day, provide a **Driver Settlement** screen
showing: - Total customer money collected. - Total amount due to the
shop. - Total amount already remitted. - Remaining cash/card/UPI
collections to reconcile. - Driver compensation earned. - Compensation
already paid by the shop. - Net amount payable or receivable, clearly
showing how it was calculated.

Do not silently net unrelated amounts. Show gross collections,
remittances, and compensation separately even if the business later
settles a net amount.

### 26.7 Suggested additional data entities

Add or extend these entities:

  ---------------------------------------------------------------------
  Entity                             Purpose
  ---------------------------------- ----------------------------------
  `delivery_payment_arrangements`    Immutable per-delivery snapshot of
                                     who collects, who pays, fee and
                                     compensation terms

  `delivery_collections`             Customer payments collected by
                                     shop or driver

  `driver_remittances`               Money handed by driver to shop

  `driver_compensations`             Amounts earned/payable to drivers
                                     for completed delivery jobs

  `driver_settlement_batches`        Groups a driver's collections and
                                     compensation for a shift or period

  `driver_settlement_lines`          Individual collection, remittance,
                                     or compensation entries in a
                                     settlement

  `payment_allocations`              Allocates received customer
                                     payments to the sale balance, if
                                     partial payments are supported
  ---------------------------------------------------------------------

All money events should reference the relevant sale, delivery, driver,
actor, payment method, amount, timestamp, and a unique
transaction/reference identifier where applicable.

### 26.8 Accounting and integrity rules

1.  A delivery being marked `delivered` does not automatically mean
    customer payment has been received.
2.  Customer payment collection, driver remittance, and driver
    compensation are separate events.
3.  Never mark a customer balance paid based only on a driver status
    update.
4.  Record who physically received each payment and by what method.
5.  Prevent duplicate collection and settlement submissions using
    idempotency keys.
6.  A partial payment or partial remittance must remain visibly
    outstanding.
7.  Driver settlement must be auditable and should not delete or
    overwrite prior entries.
8.  If a collection is corrected, create a reversal/adjustment entry
    with a reason and actor rather than editing the original financial
    event.
9.  Do not allow a driver to approve their own cash remittance or
    compensation payout if the business requires manager verification.
10. Restrict drivers to their own delivery and settlement records.
11. A cancelled or failed delivery must follow an explicit rule for
    customer refunds, goods return, and any driver compensation.
12. The sale/payment ledger remains the source of truth for customer
    amounts; the driver settlement ledger tracks custody and transfer of
    money.

### 26.9 Delivery and payment settings

Provide business defaults for: - Default customer payment collector. -
Default customer payment methods. - Default delivery fee policy. -
Whether drivers may retain delivery fees. - Whether drivers must remit
all customer collections. - Default driver compensation rule. - Whether
driver compensation is paid per completed delivery or by shift. -
Whether manager approval is required for settlements.

Defaults should prefill new orders, but an authorized manager should be
able to select a different arrangement for an individual delivery. The
applied arrangement must be snapshotted and visible before dispatch.

### 26.10 Additional API endpoints

  -----------------------------------------------------------------------
  Purpose                       Endpoint
  ----------------------------- -----------------------------------------
  Record customer collection    `POST /deliveries/{id}/collections`

  Record driver remittance      `POST /driver-remittances`

  List driver's settlement      `GET /me/settlements/summary`
  summary                       

  List settlement entries       `GET /me/settlements`

  Create settlement batch       `POST /driver-settlements`

  Verify/approve settlement     `POST /driver-settlements/{id}/approve`

  Record driver compensation    `POST /driver-compensations/{id}/pay`
  payment                       

  Configure payment defaults    `PATCH /settings/delivery-payments`
  -----------------------------------------------------------------------

The API must validate that the authenticated actor is allowed to record
each event. A driver may report a collection or handover, but a manager
or authorized cashier should verify receipt of money by the shop where
required.

### 26.11 Acceptance criteria

The delivery payment feature is complete when: - A manager can choose
the customer payment collector per delivery. - A manager can choose
whether the customer pays the delivery fee directly to the driver or the
shop. - The system can record driver compensation separately from
customer delivery charges. - The driver can see the exact amount to
collect and/or remit. - The shop can record and verify remittances and
compensation payouts. - Partial payments and partial remittances remain
outstanding. - A delivery can be completed without incorrectly marking
its payment or driver settlement as complete. - Reports reconcile
customer collections, shop receipts, driver remittances, and driver
compensation independently.

## 27. Driver engagement model and cash-return workflow

### 27.1 Driver may collect the full amount and return it to the shop

The delivery model must support a third collection arrangement in
addition to the earlier examples:

-   Driver collects the full customer amount (merchandise plus delivery
    fee) and returns the **entire collection** to the shop.
-   The shop separately pays the driver an agreed delivery compensation.
-   The driver does not retain the delivery fee unless the specific
    delivery arrangement explicitly says so.

Example: - Merchandise: ₹1,000 - Customer delivery fee: ₹70 - Driver
collects: ₹1,070 - Driver returns to shop: ₹1,070 - Shop pays driver
compensation: ₹50 - Driver net compensation after settlement: ₹50

This is distinct from a driver-retains-fee arrangement. Never assume
that the customer delivery fee equals the driver's compensation.

### 27.2 Represent the arrangement as explicit money-flow instructions

Avoid a growing collection of loosely related booleans. Use explicit,
validated choices:

**Customer payment collector** - `shop` - `driver`

**Driver collection disposition** -
`retain_delivery_fee_and_remit_balance` -
`remit_full_collection_to_shop` - `not_applicable`

**Delivery fee payer** - `customer` - `shop_waived`

**Driver compensation basis** - `per_delivery` - `fixed_amount` - `none`

**Driver compensation payer** - `shop` - `not_applicable`

The backend should validate combinations. For example,
`remit_full_collection_to_shop` means the driver owes the shop the
entire amount collected. Driver compensation remains a separate payable
from the shop and must not be silently netted against the remittance.

A delivery's payment arrangement must be snapshotted when confirmed.
Changing business defaults affects future deliveries only.

### 27.3 Example money-flow scenarios

  -----------------------------------------------------------------------
  Scenario            Customer pays  Driver returns to   Shop pays driver
                                                  shop 
  -------------- ------------------ ------------------ ------------------
  Driver retains             ₹1,070             ₹1,000      ₹0 separately
  agreed                                               
  delivery fee                                         

  Driver remits              ₹1,070             ₹1,070   ₹50 compensation
  full                                                 
  collection                                           

  Shop collects      ₹1,070 to shop                 ₹0   ₹50 compensation
  before                                               
  dispatch                                             

  Shop waives                ₹1,000         Depends on             Agreed
  delivery fee                               collector      compensation,
                                                                 e.g. ₹50
  -----------------------------------------------------------------------

All amounts are examples. The delivery fee charged to the customer and
compensation paid to the driver are separate values and need not be
equal.

### 27.4 Settlement controls

-   Record every collection as a payment/custody event with collector,
    method, amount, delivery, and timestamp.
-   Record every handover to the shop as a separate remittance event.
-   Require a shop-side user to acknowledge physical cash handover where
    practical.
-   Record driver compensation as a separate payable and payout event.
-   Support partial remittance and partial compensation payment.
-   Show gross collections, remittances, and compensation separately.
-   Do not silently offset the driver's compensation against money
    collected for the shop.
-   Use idempotency keys to prevent duplicate collection or remittance
    entries.
-   Reconcile cash, UPI, and other payment methods separately.
-   Restrict drivers to reporting their own collections and handovers;
    authorized shop staff verify receipt.

### 27.5 Driver engagement and legal/compliance boundary

The application should support a lawful independent delivery-provider
workflow, but software configuration cannot make an otherwise unlawful
transport or employment arrangement lawful. Do not use labels such as
"acquaintance," "informal helper," "cash allowance," or "independent
contractor" to disguise a relationship that legally requires a permit,
commercial insurance, registration, licence, tax treatment, or
employment protections.

Before enabling a driver or vehicle, the business should verify the
requirements that apply to the actual service and jurisdiction with the
competent Mizoram transport authority and a local legal/accounting
professional. In particular, confirm: - Whether the proposed activity is
legally treated as delivery of goods by a two-wheeler, passenger
transport, a bike-taxi/aggregator service, or another regulated
category. These are not automatically the same activity. - Whether the
vehicle's registration class and permitted use cover the intended
commercial delivery activity. - Whether the motor insurance policy
expressly covers the intended commercial use and delivery work; ordinary
private-use cover may not. - Whether the rider has the appropriate,
valid driving licence and the vehicle has required registration, tax,
fitness/PUC and safety compliance. - Whether any local permit, trade,
municipal, transport, aggregator, or other authorisation applies. -
Whether the business must register, maintain records, or meet
worker/contractor obligations under applicable central and state law. -
Whether accident, goods-in-transit, public liability, or other insurance
is appropriate.

Do not assume that a two-wheeler taxi driver's passenger-service
authorisation automatically covers carrying retail goods, or vice versa.
Ask the transport authority/insurer about the exact activity and vehicle
use.

### 27.6 Practical compliant operating models to evaluate

The business can evaluate these options with local professional advice:

1.  **Use an established licensed delivery/courier operator.** Contract
    with a provider whose vehicles, riders, insurance, and
    authorisations cover the service being purchased.
2.  **Engage individual delivery providers transparently.** Use a
    written service agreement describing the delivery service, rates,
    payment collection, remittance, responsibility for goods, accident
    reporting, and proof of delivery. Verify the rider, vehicle
    documents, insurance and any required authorisations before
    activation.
3.  **Use a properly authorised business fleet.** If delivery becomes
    regular and material, consider vehicles registered and insured for
    the relevant commercial use, with the required permits and operating
    arrangements.

A written agreement is useful evidence of the parties' actual terms, but
it does not override statutory requirements or determine worker status
by itself. The actual working relationship and applicable law matter.

### 27.7 Driver and vehicle records

Add records for: - Driver identity and contact details. - Driving
licence verification status and expiry date. - Vehicle registration
number and registration verification status. - Registered owner and the
driver's authority to use the vehicle. - Insurance provider, policy
number, coverage type, and expiry date. - PUC/fitness/permit details
where applicable. - Document verification date, verifier, and next
review date. - Service agreement acceptance and effective dates.

Store only necessary document data, restrict access, encrypt sensitive
documents at rest where feasible, and define retention/deletion rules.
Do not expose full identity or licence documents to ordinary dispatch
users.

Prevent assignment to a driver/vehicle whose required verification is
expired or incomplete. Provide an administrative override only if
legally permissible, permission-controlled, reasoned, and audited; do
not allow an override to bypass a statutory prohibition.

### 27.8 Delivery provider agreement checklist

Have a local professional prepare or review the agreement. It should
address: - Scope of delivery services and service area. - Whether the
provider uses their own vehicle. - Vehicle and insurance
responsibilities. - Required licences, permits, and document updates. -
Delivery fee versus driver compensation. - Customer payment collection
and full/partial remittance deadlines. - Cash/UPI handling, receipts,
reconciliation, and shortage/dispute process. - Responsibility for goods
while in custody. - Failed deliveries, returns, damage, and customer
complaints. - Accident, injury, and incident reporting. - Data privacy
and customer contact restrictions. - Termination, suspension, and
dispute resolution.

Do not present this checklist as a substitute for legal review.

### 27.9 Product acceptance criteria

-   The system supports driver-retains-fee, full-remittance-to-shop, and
    shop-collects/pays-driver arrangements.
-   Customer delivery fee and driver compensation are independently
    configurable.
-   Full driver collections can be recorded and reconciled against full
    shop remittances.
-   Driver compensation is separately payable and auditable.
-   Driver and vehicle verification status is visible to authorised
    administrators.
-   Expired or unverified required documents block ordinary assignment.
-   Drivers see only their own assigned deliveries and relevant
    settlement data.

## 28. Part-time driver pool and delivery offer board

### 28.1 Operating model

The intended driver model is a pool of part-time delivery providers.
These individuals already own or have access to two-wheelers and want to
earn additional income by accepting occasional delivery work. They are
not assumed to be full-time staff or to be continuously available.

The application should therefore support an **opt-in delivery offer
board** rather than relying exclusively on a manager assigning every job
to a fixed driver.

Registration gives an approved driver access to eligible delivery
opportunities. Registration does not mean that the driver is always
available, is guaranteed work, or must accept every offer. The actual
legal relationship must still be assessed against applicable law and the
real working arrangement; the software must not claim that a person is
an independent contractor merely because they accepted an app invitation
or signed a template agreement.

### 28.2 Driver onboarding and approval

Suggested onboarding states:

-   `application_started`
-   `documents_submitted`
-   `under_review`
-   `approved`
-   `suspended`
-   `rejected`
-   `deactivated`

Before a driver can view or accept live delivery offers, an authorised
administrator should: 1. Verify identity and contact details to the
extent necessary. 2. Verify driving licence and relevant vehicle
documents. 3. Verify that the vehicle's registration, insurance and
permitted use cover the proposed delivery activity. 4. Confirm any
permit/authorisation requirements with the competent authority where
applicable. 5. Record the service agreement and its effective date, if a
professional confirms that agreement is appropriate. 6. Set the driver's
service area and any operational restrictions. 7. Approve the driver
account.

A driver with expired, incomplete, or unverified mandatory documents
must not be able to accept jobs. The system should notify administrators
before document expiry.

### 28.3 Availability

Drivers should control whether they are currently available: -
`offline` - `available` - `busy` - `paused`

A driver can switch to `available` when willing to receive offers and
`offline` when not working. Availability is an operational signal, not a
promise to accept a job.

For the initial release, availability can be a simple toggle. Do not
implement continuous GPS tracking. Location should be requested only
when needed for a specific delivery workflow and with clear notice.

### 28.4 Delivery offer board

When an order is packed and ready for dispatch, the system creates a
delivery offer. Eligible approved drivers may see a limited offer card.

Before accepting, show only the information needed to decide whether to
take the job: - Approximate locality or delivery zone (not the
customer's full address). - Pickup locality/store. - Estimated delivery
distance or zone, if known. - Delivery fee/driver compensation for that
job. - Expected collection/remittance arrangement. - Item handling notes
(e.g. fragile or bulky), without unnecessary customer details. - Offer
expiry time, if offers expire.

Do not reveal the customer's full name, phone number, precise address,
OTP, or other personal data to every driver browsing the offer board.
Reveal the necessary delivery details only to the driver whose
acceptance has been confirmed.

### 28.5 Accepting and assigning a delivery

Recommended first-release workflow:

1.  Manager or cashier marks the packed order `ready_for_dispatch`.
2.  The system creates an offer visible to eligible available drivers in
    the configured service area.
3.  A driver reviews the offer and chooses to accept or decline.
4.  The backend performs an atomic assignment so that only one driver
    can win the offer.
5.  Once assigned, the driver receives the full address, recipient
    contact details, delivery instructions, and collection requirements.
6.  Other drivers see that the offer is no longer available.
7.  The driver can proceed through pickup, out-for-delivery, delivered,
    or failed states.

If two drivers accept at nearly the same time, the backend must use a
transaction/conditional update or equivalent concurrency control. Only
one acceptance may succeed. The other driver receives a clear "already
assigned" response.

A manager should retain the ability to manually assign or reassign a
delivery. Reassignment must preserve assignment history and revoke the
previous driver's access to customer details and active delivery
actions.

### 28.6 Offer expiry and unclaimed deliveries

An offer may expire after a configurable period. If no driver accepts: -
Keep the delivery in `ready_for_dispatch`. - Notify dispatch staff. -
Allow staff to re-offer it, contact a driver, or choose another delivery
provider. - Do not silently cancel the customer's order.

Avoid repeatedly notifying every driver without rate limits. Provide an
administrative fallback for urgent or bulky deliveries.

### 28.7 Driver compensation visibility

Show the driver the compensation for the job before acceptance. Clearly
distinguish: - Delivery charge paid by the customer. - Compensation the
driver earns. - Money the driver must collect from the customer. - Money
the driver must return to the shop. - Any amount the shop will pay the
driver separately.

The driver must not have to infer earnings from the customer delivery
fee. The offer should display the exact compensation arrangement.

### 28.8 Suggested data model additions

Add:

  -----------------------------------------------------------------------
  Entity/field                        Purpose
  ----------------------------------- -----------------------------------
  `driver_profiles`                   Driver-specific onboarding and
                                      operational profile linked to a
                                      user

  `driver_documents`                  Document type, verification status,
                                      expiry and restricted storage
                                      reference

  `driver_vehicles`                   Vehicle and
                                      ownership/authorised-use details

  `driver_availability`               Current availability and optional
                                      availability history

  `delivery_offers`                   Offer status, visibility scope,
                                      expiry and delivery reference

  `delivery_offer_events`             Offered, viewed, accepted,
                                      declined, expired and withdrawn
                                      events

  `delivery_assignments`              Confirmed driver assignment history
  -----------------------------------------------------------------------

A delivery offer should reference the delivery rather than duplicate
customer/order information. Full recipient details should be fetched
only after a successful assignment and authorization check.

### 28.9 API outline

  Purpose                          Endpoint
  -------------------------------- --------------------------------------
  Submit driver application        `POST /driver/application`
  Get own onboarding status        `GET /me/driver-profile`
  Update availability              `PATCH /me/driver-availability`
  List eligible offers             `GET /me/delivery-offers`
  View offer summary               `GET /delivery-offers/{id}`
  Accept offer                     `POST /delivery-offers/{id}/accept`
  Decline offer                    `POST /delivery-offers/{id}/decline`
  List own active jobs             `GET /me/deliveries`
  Admin approve/suspend driver     `POST /admin/drivers/{id}/status`
  Admin create/re-offer delivery   `POST /deliveries/{id}/offer`

All `/me` endpoints must derive the driver identity from the
authenticated session, not from a user ID supplied by the client. The
accept endpoint must atomically claim the offer and verify that the
driver is approved, available, eligible for the service area, and has
current required documents.

### 28.10 Privacy and safety

-   Do not expose full customer details to all registered drivers.
-   Reveal customer information only after assignment and only for the
    active delivery.
-   Revoke access when a job is reassigned, cancelled, or completed,
    subject to legitimate record-retention needs.
-   Do not continuously track drivers by default.
-   If location sharing is introduced, request it only during an active
    delivery, explain the purpose, and set a retention policy.
-   Keep delivery proof proportionate; an OTP or recipient confirmation
    may be sufficient.
-   Restrict access to driver identity, licence, insurance, and vehicle
    documents.
-   Log access to sensitive delivery and driver records.

### 28.11 Legal and operational qualification

The part-time, opt-in nature of the pool is a business intention, not a
legal conclusion. Whether a person is legally an employee, worker,
contractor, agent, or transport operator depends on applicable law and
the actual facts, including the degree of control, regularity, payment,
obligations, and manner of work. App registration and a written contract
do not decide that question by themselves.

Before launch, obtain local advice on: - Whether this goods-delivery
activity requires a particular vehicle registration, permit, licence,
insurance endorsement, or business authorisation in Mizoram. - Whether
an existing two-wheeler taxi authorisation covers goods delivery (do not
assume it does). - The correct legal and tax treatment of part-time
delivery providers and payments. - Responsibility for accidents,
customer goods, and third-party claims.

The product must record accurate facts and support compliant operations.
It must not be designed to disguise employment, evade permits, or bypass
insurance requirements.

### 28.12 Acceptance criteria

-   An applicant cannot view or accept offers until approved.
-   Approved drivers can opt in and out of availability.
-   Drivers see a limited offer summary before acceptance.
-   Full customer details are revealed only to the successfully assigned
    driver.
-   Simultaneous acceptance attempts result in exactly one assignment.
-   Drivers can decline offers without being forced to accept every job.
-   Expired or suspended drivers cannot accept offers.
-   Managers can manually assign, withdraw, or reassign offers.
-   Compensation and money-collection instructions are visible before
    acceptance.
-   Assignment history and offer events are auditable.
