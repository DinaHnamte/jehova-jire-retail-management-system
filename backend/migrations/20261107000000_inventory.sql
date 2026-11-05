-- Inventory and receiving: locations, suppliers, stock ledger, goods receipts, stock counts,
-- nightly stock check. See Architecture.md §8, §9 and
-- docs/superpowers/specs/2026-11-06-phase-4-inventory-design.md.

CREATE TABLE locations (
    id         UUID PRIMARY KEY,
    code       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- One shop for now; the screens never show locations.
INSERT INTO locations (id, code, name)
VALUES ('00000000-0000-7000-8002-000000000001', 'shop', 'Shop');

CREATE TABLE suppliers (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    phone       TEXT CHECK (length(phone) <= 20),
    notes       TEXT CHECK (length(notes) <= 500),
    archived_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX suppliers_name_active_key ON suppliers (lower(name)) WHERE archived_at IS NULL;

-- Append-only stock ledger (§8.1). Quantities are signed base units.
CREATE TABLE stock_movements (
    id            UUID PRIMARY KEY,
    product_id    UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    location_id   UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    quantity      BIGINT NOT NULL CHECK (quantity <> 0),
    unit_cost     NUMERIC(12,2) CHECK (unit_cost >= 0),
    movement_type TEXT NOT NULL CHECK (movement_type IN
                  ('opening_balance', 'purchase_receipt', 'adjustment_in', 'adjustment_out', 'damage')),
    source_type   TEXT NOT NULL CHECK (source_type IN ('goods_receipt', 'stock_count', 'adjustment')),
    source_id     UUID NOT NULL,
    reason        TEXT,
    created_by    UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT stock_movements_sign CHECK (
        (movement_type IN ('opening_balance', 'purchase_receipt', 'adjustment_in') AND quantity > 0)
        OR (movement_type IN ('adjustment_out', 'damage') AND quantity < 0)
    ),
    CONSTRAINT stock_movements_reason CHECK (
        movement_type NOT IN ('adjustment_in', 'adjustment_out', 'damage') OR length(btrim(reason)) > 0
    )
);
CREATE INDEX stock_movements_product_idx ON stock_movements (product_id, created_at DESC);
CREATE INDEX stock_movements_source_idx ON stock_movements (source_type, source_id);

CREATE FUNCTION stock_movements_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'stock movements cannot be changed';
END;
$$;
CREATE TRIGGER stock_movements_no_update_delete
    BEFORE UPDATE OR DELETE ON stock_movements
    FOR EACH ROW EXECUTE FUNCTION stock_movements_immutable();

-- Cache of SUM(quantity) per product and location, kept in the same transaction as each movement.
CREATE TABLE stock_balances (
    product_id  UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    on_hand     BIGINT NOT NULL DEFAULT 0 CHECK (on_hand >= 0),
    -- Weighted-average cost; NULL means cost unknown.
    avg_cost    NUMERIC(12,4) CHECK (avg_cost >= 0),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (product_id, location_id)
);

CREATE TABLE goods_receipts (
    id           UUID PRIMARY KEY,
    supplier_id  UUID NOT NULL REFERENCES suppliers (id) ON DELETE RESTRICT,
    received_on  DATE NOT NULL,
    note         TEXT CHECK (length(note) <= 500),
    status       TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'confirmed')),
    confirmed_by UUID REFERENCES users (id) ON DELETE RESTRICT,
    confirmed_at TIMESTAMPTZ,
    created_by   UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'confirmed') = (confirmed_at IS NOT NULL))
);
CREATE INDEX goods_receipts_status_idx ON goods_receipts (status, received_on DESC);

CREATE TABLE goods_receipt_lines (
    id         UUID PRIMARY KEY,
    receipt_id UUID NOT NULL REFERENCES goods_receipts (id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    quantity   BIGINT NOT NULL CHECK (quantity > 0),
    unit_cost  NUMERIC(12,2) NOT NULL CHECK (unit_cost >= 0),
    UNIQUE (receipt_id, product_id)
);

CREATE TABLE stock_counts (
    id         UUID PRIMARY KEY,
    note       TEXT CHECK (length(note) <= 500),
    status     TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'posted')),
    posted_by  UUID REFERENCES users (id) ON DELETE RESTRICT,
    posted_at  TIMESTAMPTZ,
    created_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'posted') = (posted_at IS NOT NULL))
);
CREATE INDEX stock_counts_status_idx ON stock_counts (status, created_at DESC);

CREATE TABLE stock_count_lines (
    id         UUID PRIMARY KEY,
    count_id   UUID NOT NULL REFERENCES stock_counts (id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    counted    BIGINT NOT NULL CHECK (counted >= 0),
    unit_cost  NUMERIC(12,2) CHECK (unit_cost >= 0),
    -- On hand at the moment of posting; NULL while the count is a draft.
    expected   BIGINT,
    UNIQUE (count_id, product_id)
);

-- Nightly comparison of stock_balances with SUM(stock_movements.quantity); never auto-corrects.
CREATE TABLE stock_check_runs (
    id         UUID PRIMARY KEY,
    ran_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    mismatches JSONB NOT NULL DEFAULT '[]'
);
CREATE INDEX stock_check_runs_ran_at_idx ON stock_check_runs (ran_at DESC);
