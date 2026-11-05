-- Design/colour codes (ADR-0003, revised 5 Nov 2026): the same model in two designs
-- (IP1060-11 MP4952 pink, IP1060-11 MG4705 green) is two products. A product's identity
-- is its model code plus its design code (NULL = no design).

ALTER TABLE product_identifiers
    ADD COLUMN design_code TEXT CHECK (design_code ~ '^[A-Z0-9]{1,20}$');

-- Uniqueness now includes the design.
DO $$
DECLARE c TEXT;
BEGIN
    SELECT conname INTO c FROM pg_constraint
    WHERE conrelid = 'product_identifiers'::regclass AND contype = 'u';
    EXECUTE format('ALTER TABLE product_identifiers DROP CONSTRAINT %I', c);
END $$;
CREATE UNIQUE INDEX product_identifiers_identity_key
    ON product_identifiers (brand_id, identifier_type, normalized_value, COALESCE(design_code, ''));
CREATE INDEX product_identifiers_design_idx
    ON product_identifiers (design_code) WHERE design_code IS NOT NULL;

-- Search: a design code typed on its own is an exact match for that product.
CREATE OR REPLACE FUNCTION catalog_search_hits(raw TEXT, norm TEXT)
RETURNS TABLE (product_id UUID, stage INTEGER)
LANGUAGE sql STABLE
AS $$
    WITH args AS (
        SELECT btrim(raw) AS raw,
               norm,
               replace(replace(replace(norm, '\', '\\'), '%', '\%'), '_', '\_') || '%' AS norm_prefix,
               '%' || replace(replace(replace(btrim(raw), '\', '\\'), '%', '\%'), '_', '\_') || '%' AS raw_contains
    ),
    hits AS (
        SELECT pi.product_id,
               CASE WHEN pi.normalized_value = a.norm OR pi.design_code = a.norm THEN 1
                    WHEN pi.base_code = a.norm THEN 2
                    ELSE 3 END AS stage
        FROM product_identifiers pi, args a
        WHERE a.norm <> ''
          AND (pi.normalized_value = a.norm
               OR pi.design_code = a.norm
               OR pi.base_code = a.norm
               OR pi.normalized_value LIKE a.norm_prefix
               OR pi.digits LIKE a.norm_prefix)
        UNION ALL
        SELECT p.id, 4
        FROM products p, args a
        WHERE a.raw <> '' AND (a.raw <% p.name OR p.name ILIKE a.raw_contains)
        UNION ALL
        SELECT pi.product_id, 4
        FROM product_identifiers pi, args a
        WHERE a.raw <> '' AND pi.identifier_type = 'alias'
          AND (a.raw <% pi.raw_value OR pi.raw_value ILIKE a.raw_contains)
    )
    SELECT product_id, min(stage)::int AS stage FROM hits GROUP BY product_id
$$;
