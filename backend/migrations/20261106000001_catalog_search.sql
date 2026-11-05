-- Ranked product lookup (Architecture.md §6.4). One definition shared by
-- GET /products/search and the filtered product list.
--   raw:  what the person typed, for name matching
--   norm: the same text through the code normaliser (src/shared/code.rs)
-- Returns each matching product once, at its best stage:
--   1 exact identifier · 2 same model (base code) · 3 code or digit prefix · 4 name/alias words
CREATE FUNCTION catalog_search_hits(raw TEXT, norm TEXT)
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
               CASE WHEN pi.normalized_value = a.norm THEN 1
                    WHEN pi.base_code = a.norm THEN 2
                    ELSE 3 END AS stage
        FROM product_identifiers pi, args a
        WHERE a.norm <> ''
          AND (pi.normalized_value = a.norm
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
