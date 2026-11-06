-- Returns (Phase 5b): a return records goods coming back against an order, split into the part
-- that reduced what the customer owed and the part refunded in cash or UPI. Rows are immutable.
-- Also opens the stock_movements types for cancel and return.

CREATE TABLE returns (
    id            UUID PRIMARY KEY,
    order_id      UUID NOT NULL REFERENCES orders (id) ON DELETE RESTRICT,
    refund_method TEXT CHECK (refund_method IN ('cash', 'upi')),
    amount        NUMERIC(12,2) NOT NULL CHECK (amount > 0),
    owed_reduced  NUMERIC(12,2) NOT NULL CHECK (owed_reduced >= 0),
    refunded      NUMERIC(12,2) NOT NULL CHECK (refunded >= 0),
    note          TEXT CHECK (length(note) <= 500),
    created_by    UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT returns_amounts_add_up CHECK (owed_reduced + refunded = amount),
    CONSTRAINT returns_method_iff_refunded CHECK ((refunded > 0) = (refund_method IS NOT NULL))
);
CREATE INDEX returns_order_idx ON returns (order_id);

CREATE TABLE return_lines (
    id            UUID PRIMARY KEY,
    return_id     UUID NOT NULL REFERENCES returns (id) ON DELETE RESTRICT,
    order_line_id UUID NOT NULL REFERENCES order_lines (id) ON DELETE RESTRICT,
    quantity      BIGINT NOT NULL CHECK (quantity > 0),
    disposition   TEXT NOT NULL CHECK (disposition IN ('sellable', 'damaged')),
    amount        NUMERIC(12,2) NOT NULL CHECK (amount >= 0)
);
CREATE INDEX return_lines_order_line_idx ON return_lines (order_line_id);

CREATE TRIGGER returns_immutable
    BEFORE UPDATE OR DELETE ON returns
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER return_lines_immutable
    BEFORE UPDATE OR DELETE ON return_lines
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();

-- Stock: cancelling a sale and taking a return both put goods back (inbound sign).
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_movement_type_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_movement_type_check CHECK (movement_type IN
    ('opening_balance', 'purchase_receipt', 'adjustment_in', 'adjustment_out', 'damage', 'sale',
     'sale_cancel', 'sale_return'));
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_source_type_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_source_type_check CHECK (source_type IN
    ('goods_receipt', 'stock_count', 'adjustment', 'order', 'return'));
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_sign;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_sign CHECK (
    (movement_type IN ('opening_balance', 'purchase_receipt', 'adjustment_in', 'sale_cancel', 'sale_return')
        AND quantity > 0)
    OR (movement_type IN ('adjustment_out', 'damage', 'sale') AND quantity < 0)
);
