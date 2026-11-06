-- Phase 6a: deliveries. Orders carry a delivery fee; drivers, deliveries,
-- assignments, attempts and events; new ledger account/kind and stock source.

-- Orders
ALTER TABLE orders DROP CONSTRAINT orders_fulfilment_check;
ALTER TABLE orders ADD CONSTRAINT orders_fulfilment_check CHECK (fulfilment IN ('counter', 'delivery'));
ALTER TABLE orders ADD COLUMN delivery_fee NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (delivery_fee >= 0);
ALTER TABLE orders DROP CONSTRAINT orders_total_matches;
ALTER TABLE orders ADD CONSTRAINT orders_total_matches
    CHECK (total = subtotal - order_discount + delivery_fee AND total >= 0);
ALTER TABLE orders ADD CONSTRAINT orders_fee_only_on_delivery
    CHECK (fulfilment = 'delivery' OR delivery_fee = 0);

-- Ledger
ALTER TABLE ledger_entries DROP CONSTRAINT ledger_entries_account_check;
ALTER TABLE ledger_entries ADD CONSTRAINT ledger_entries_account_check CHECK (account IN
    ('receivable', 'cash', 'upi', 'sales_revenue', 'sales_returns', 'delivery_fee_income'));
ALTER TABLE ledger_transactions DROP CONSTRAINT ledger_transactions_kind_check;
ALTER TABLE ledger_transactions ADD CONSTRAINT ledger_transactions_kind_check CHECK (kind IN
    ('sale', 'payment', 'cancel', 'refund', 'return_credit', 'fee_adjustment'));

-- Stock: check-in writes damage movements sourced from a delivery.
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_source_type_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_source_type_check CHECK (source_type IN
    ('goods_receipt', 'stock_count', 'adjustment', 'order', 'return', 'delivery'));

CREATE TABLE driver_profiles (
    user_id     UUID PRIMARY KEY REFERENCES users (id) ON DELETE RESTRICT,
    phone       TEXT NOT NULL UNIQUE CHECK (phone ~ '^[6-9][0-9]{9}$'),
    default_pay NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (default_pay >= 0),
    note        TEXT CHECK (length(note) <= 500),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Active/inactive is users.is_active; there is no second flag.

CREATE TABLE deliveries (
    id                UUID PRIMARY KEY,
    order_id          UUID NOT NULL UNIQUE REFERENCES orders (id) ON DELETE RESTRICT,
    status            TEXT NOT NULL CHECK (status IN
                      ('to_send', 'assigned', 'picked_up', 'delivered', 'failed', 'returned', 'cancelled')),
    recipient_name    TEXT NOT NULL CHECK (length(recipient_name) BETWEEN 1 AND 120),
    recipient_phone   TEXT NOT NULL CHECK (recipient_phone ~ '^[6-9][0-9]{9}$'),
    address_line      TEXT NOT NULL CHECK (length(address_line) BETWEEN 1 AND 300),
    locality          TEXT NOT NULL CHECK (length(locality) BETWEEN 1 AND 80),
    landmark          TEXT CHECK (length(landmark) <= 120),
    instructions      TEXT CHECK (length(instructions) <= 300),
    fee               NUMERIC(12,2) NOT NULL CHECK (fee >= 0),
    driver_pay        NUMERIC(12,2) CHECK (driver_pay >= 0),   -- NULL until assignment at the latest
    collector         TEXT NOT NULL CHECK (collector IN ('shop', 'driver')),
    pay_method        TEXT NOT NULL CHECK (pay_method IN ('retain_from_collection', 'paid_by_shop')),
    current_driver_id UUID REFERENCES users (id) ON DELETE RESTRICT,
    received_by       TEXT CHECK (length(received_by) BETWEEN 1 AND 80),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    assigned_at       TIMESTAMPTZ,
    picked_up_at      TIMESTAMPTZ,
    delivered_at      TIMESTAMPTZ,
    closed_at         TIMESTAMPTZ,   -- returned or cancelled
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (pay_method <> 'retain_from_collection' OR collector = 'driver'),
    CHECK ((status IN ('assigned', 'picked_up')) = (current_driver_id IS NOT NULL)),
    CHECK (status NOT IN ('assigned', 'picked_up', 'delivered') OR driver_pay IS NOT NULL),
    CHECK ((status = 'delivered') = (received_by IS NOT NULL))
);
CREATE INDEX deliveries_status_idx ON deliveries (status, created_at DESC);
CREATE INDEX deliveries_driver_idx ON deliveries (current_driver_id) WHERE current_driver_id IS NOT NULL;

CREATE TABLE delivery_assignments (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    driver_id   UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    assigned_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at    TIMESTAMPTZ,
    end_reason  TEXT CHECK (end_reason IN ('reassigned', 'completed', 'failed', 'cancelled')),
    CHECK ((ended_at IS NULL) = (end_reason IS NULL))
);
CREATE UNIQUE INDEX delivery_assignments_one_open ON delivery_assignments (delivery_id) WHERE ended_at IS NULL;
CREATE INDEX delivery_assignments_driver_idx ON delivery_assignments (driver_id, assigned_at DESC);

CREATE TABLE delivery_attempts (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    driver_id   UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    reason      TEXT NOT NULL CHECK (reason IN ('customer_unavailable', 'address_not_found',
                'customer_refused', 'unreachable_by_phone', 'other')),
    note        TEXT CHECK (length(note) <= 500),
    trip_pay    NUMERIC(12,2) CHECK (trip_pay >= 0),   -- set when the owner retries or checks in
    recorded_by UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (reason <> 'other' OR note IS NOT NULL)
);

CREATE TABLE delivery_events (
    id          UUID PRIMARY KEY,
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE RESTRICT,
    from_status TEXT,
    to_status   TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('status', 'edit')),
    actor_id    UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    note        TEXT CHECK (length(note) <= 500),
    changes     JSONB,   -- for 'edit': {field: [old, new]}
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX delivery_events_delivery_idx ON delivery_events (delivery_id, occurred_at);

CREATE TRIGGER delivery_events_immutable
    BEFORE UPDATE OR DELETE ON delivery_events
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
