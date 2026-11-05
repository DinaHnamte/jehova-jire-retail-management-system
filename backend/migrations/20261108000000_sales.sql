-- Sales, customers and the money ledger (Phase 5a): customers, receipt numbering, orders,
-- order lines, double-entry ledger, idempotency keys. See
-- docs/superpowers/specs/2026-11-06-phase-5-pos-design.md §1 and ADR-0004 / ADR-0006.

CREATE TABLE customers (
    id         UUID PRIMARY KEY,
    phone      TEXT NOT NULL UNIQUE CHECK (phone ~ '^[6-9][0-9]{9}$'),
    name       TEXT CHECK (length(name) <= 120),
    note       TEXT CHECK (length(note) <= 500),
    created_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Gap-free numbering per document type and fiscal year, taken inside the checkout transaction.
CREATE TABLE document_sequences (
    doc_type    TEXT NOT NULL,
    fiscal_year TEXT NOT NULL,
    next_value  BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (doc_type, fiscal_year)
);

CREATE TABLE orders (
    id             UUID PRIMARY KEY,
    number         TEXT NOT NULL UNIQUE,
    customer_id    UUID REFERENCES customers (id) ON DELETE RESTRICT,
    fulfilment     TEXT NOT NULL DEFAULT 'counter' CHECK (fulfilment IN ('counter')),
    status         TEXT NOT NULL CHECK (status IN ('confirmed', 'cancelled')),
    subtotal       NUMERIC(12,2) NOT NULL CHECK (subtotal >= 0),
    order_discount NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (order_discount >= 0),
    total          NUMERIC(12,2) NOT NULL,
    note           TEXT CHECK (length(note) <= 500),
    receipt_token  TEXT NOT NULL UNIQUE,
    created_by     UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Filled by cancel (5b).
    cancelled_at   TIMESTAMPTZ,
    cancelled_by   UUID REFERENCES users (id) ON DELETE RESTRICT,
    cancel_reason  TEXT,
    CONSTRAINT orders_total_matches CHECK (total = subtotal - order_discount AND total >= 0)
);
CREATE INDEX orders_created_at_idx ON orders (created_at DESC);
CREATE INDEX orders_customer_idx ON orders (customer_id, created_at);

CREATE TABLE order_lines (
    id                   UUID PRIMARY KEY,
    order_id             UUID NOT NULL REFERENCES orders (id) ON DELETE RESTRICT,
    line_no              INT NOT NULL,
    product_id           UUID NOT NULL REFERENCES products (id) ON DELETE RESTRICT,
    product_name         TEXT NOT NULL,
    product_code         TEXT,
    quantity             BIGINT NOT NULL CHECK (quantity > 0),
    list_price           NUMERIC(12,2) NOT NULL CHECK (list_price >= 0),
    unit_price           NUMERIC(12,2) NOT NULL CHECK (unit_price >= 0),
    line_discount        NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (line_discount >= 0),
    order_discount_share NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (order_discount_share >= 0),
    net_amount           NUMERIC(12,2) NOT NULL,
    tax_rate             NUMERIC(5,2) NOT NULL DEFAULT 0,
    tax_amount           NUMERIC(12,2) NOT NULL DEFAULT 0,
    unit_cost            NUMERIC(12,4) CHECK (unit_cost >= 0),
    UNIQUE (order_id, line_no),
    CONSTRAINT order_lines_net_matches CHECK (
        net_amount = quantity * unit_price - line_discount - order_discount_share AND net_amount >= 0
    )
);
CREATE INDEX order_lines_product_idx ON order_lines (product_id);

-- Double-entry money ledger (Architecture.md §14). Debit positive, credit negative.
CREATE TABLE ledger_transactions (
    id              UUID PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('sale', 'payment', 'cancel', 'refund', 'return_credit')),
    order_id        UUID REFERENCES orders (id) ON DELETE RESTRICT,
    customer_id     UUID REFERENCES customers (id) ON DELETE RESTRICT,
    method          TEXT CHECK (method IN ('cash', 'upi', 'internal')),
    received_by     UUID REFERENCES users (id) ON DELETE RESTRICT,
    external_ref    TEXT,
    reverses_id     UUID REFERENCES ledger_transactions (id) ON DELETE RESTRICT,
    reason          TEXT,
    note            TEXT CHECK (length(note) <= 500),
    actor_id        UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    idempotency_key TEXT NOT NULL UNIQUE,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ledger_transactions_occurred_idx ON ledger_transactions (occurred_at);

CREATE TABLE ledger_entries (
    id             UUID PRIMARY KEY,
    transaction_id UUID NOT NULL REFERENCES ledger_transactions (id) ON DELETE RESTRICT,
    account        TEXT NOT NULL CHECK (account IN
                   ('receivable', 'cash', 'upi', 'sales_revenue', 'sales_returns')),
    ref_id         UUID,
    amount         NUMERIC(12,2) NOT NULL CHECK (amount <> 0),
    -- A receivable entry always names its order; no other account does.
    CHECK ((account = 'receivable') = (ref_id IS NOT NULL))
);
CREATE INDEX ledger_entries_transaction_idx ON ledger_entries (transaction_id);
CREATE INDEX ledger_entries_account_idx ON ledger_entries (account, ref_id);

CREATE FUNCTION ledger_check_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (SELECT COALESCE(SUM(amount), 0) FROM ledger_entries WHERE transaction_id = NEW.transaction_id) <> 0 THEN
        RAISE EXCEPTION 'ledger transaction does not balance';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER ledger_balanced
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION ledger_check_balanced();

CREATE FUNCTION ledger_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% cannot be changed', TG_TABLE_NAME;
END;
$$;
CREATE TRIGGER ledger_transactions_immutable
    BEFORE UPDATE OR DELETE ON ledger_transactions
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER ledger_entries_immutable
    BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER order_lines_immutable
    BEFORE UPDATE OR DELETE ON order_lines
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();

-- Replay protection for checkout and customer payments.
CREATE TABLE idempotency_keys (
    key          TEXT NOT NULL,
    user_id      UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    scope        TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response     JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, scope, key)
);

-- Stock: sales leave the shop (outbound sign); cancel and return types wait for 5b.
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_movement_type_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_movement_type_check CHECK (movement_type IN
    ('opening_balance', 'purchase_receipt', 'adjustment_in', 'adjustment_out', 'damage', 'sale'));
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_source_type_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_source_type_check CHECK (source_type IN
    ('goods_receipt', 'stock_count', 'adjustment', 'order'));
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_sign;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_sign CHECK (
    (movement_type IN ('opening_balance', 'purchase_receipt', 'adjustment_in') AND quantity > 0)
    OR (movement_type IN ('adjustment_out', 'damage', 'sale') AND quantity < 0)
);

INSERT INTO settings (key, value) VALUES ('shop_name', '"Jehovah Jireh"') ON CONFLICT DO NOTHING;
