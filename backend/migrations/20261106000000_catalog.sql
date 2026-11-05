-- Catalogue: brands, categories, products, identifiers. See Architecture.md §6, §14; ADR-0003.

CREATE TABLE brands (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX brands_name_key ON brands (lower(name));

CREATE TABLE categories (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX categories_name_key ON categories (lower(name));

CREATE TABLE products (
    id            UUID PRIMARY KEY,
    brand_id      UUID NOT NULL REFERENCES brands (id) ON DELETE RESTRICT,
    category_id   UUID REFERENCES categories (id) ON DELETE RESTRICT,
    name          TEXT NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    description   TEXT,
    unit          TEXT NOT NULL DEFAULT 'piece'
                  CHECK (unit IN ('piece', 'set', 'pack', 'box', 'pair', 'dozen')),
    pack_qty      INTEGER NOT NULL DEFAULT 1 CHECK (pack_qty > 0),
    attributes    JSONB NOT NULL DEFAULT '{}',
    selling_price NUMERIC(12,2) NOT NULL CHECK (selling_price >= 0),
    mrp           NUMERIC(12,2) CHECK (mrp >= 0),
    default_cost  NUMERIC(12,2) CHECK (default_cost >= 0),
    hsn_code      TEXT,
    tax_category  TEXT,
    archived_at   TIMESTAMPTZ,
    created_by    UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Selling above MRP is not allowed (ADR-0003).
    CHECK (mrp IS NULL OR selling_price <= mrp)
);
CREATE INDEX products_archived_at_idx ON products (archived_at);
CREATE INDEX products_category_idx ON products (category_id);
CREATE INDEX products_name_trgm_idx ON products USING gin (name gin_trgm_ops);

CREATE TABLE product_identifiers (
    id               UUID PRIMARY KEY,
    product_id       UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    brand_id         UUID NOT NULL REFERENCES brands (id) ON DELETE RESTRICT,
    identifier_type  TEXT NOT NULL CHECK (identifier_type IN ('product_code', 'sku', 'barcode', 'alias')),
    raw_value        TEXT NOT NULL,
    normalized_value TEXT NOT NULL CHECK (normalized_value <> ''),
    base_code        TEXT,
    variant          TEXT,
    -- Numeric size for sorting (B6143-8 before B6143-10); NULL when not a number.
    variant_num      NUMERIC,
    -- Code without its letter prefix, for digit-prefix search ("614" finds B6143-*).
    digits           TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (brand_id, identifier_type, normalized_value)
);
CREATE UNIQUE INDEX product_identifiers_one_code_per_product
    ON product_identifiers (product_id) WHERE identifier_type = 'product_code';
CREATE INDEX product_identifiers_product_idx ON product_identifiers (product_id);
CREATE INDEX product_identifiers_normalized_prefix_idx
    ON product_identifiers (normalized_value text_pattern_ops);
CREATE INDEX product_identifiers_digits_prefix_idx
    ON product_identifiers (digits text_pattern_ops);
CREATE INDEX product_identifiers_base_idx ON product_identifiers (base_code);
CREATE INDEX product_identifiers_normalized_trgm_idx
    ON product_identifiers USING gin (normalized_value gin_trgm_ops);

INSERT INTO brands (id, name) VALUES ('00000000-0000-7000-8001-000000000001', 'Ektra by Superware');
