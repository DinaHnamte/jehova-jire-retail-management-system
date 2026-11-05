// Product code normalisation (Architecture.md §6.3, ADR-0003).
// Mirrors backend/src/shared/code.rs; both run fixtures/codes.json.

export interface ProductCode {
  /** `B6275-4.75`: the stored, compared form. */
  normalized: string
  /** `B6275`: shared by every size of a model. */
  base: string
  /** `4.75`: the size part, if any. */
  variant: string | null
  /** `6275-4.75`: without the letter prefix, for digit-prefix search. */
  digits: string
}

const DASHES = /[‐‑‒–—―−﹣－]/g
// Inch marks and look-alikes after a size. NFKC turns ″ into ′′.
const TRAILING_INCH = /["'″′”’`]+$/

/** Normalise free text the same way codes are: never fails, may return "". */
export function normalizeQuery(q: string): string {
  return q
    .normalize('NFKC')
    .replace(DASHES, '-')
    .replace(/\s+/g, '')
    .toUpperCase()
    .replace(TRAILING_INCH, '')
    .replace(/^-+|-+$/g, '')
    // `IP-1060-11` is `IP 1060-11` typed with a hyphen: drop hyphens between the letter
    // prefix and the first digit so it can't become a different code.
    .replace(/^(\p{L}+)-+(?=[0-9])/u, '$1')
}

/** Normalise a printed code. `null` if nothing code-like is left (empty or no digit). */
export function normalizeCode(raw: string): ProductCode | null {
  const normalized = normalizeQuery(raw)
  if (!normalized || !/[0-9]/.test(normalized)) return null
  const i = normalized.indexOf('-')
  const base = i === -1 ? normalized : normalized.slice(0, i)
  const rest = i === -1 ? '' : normalized.slice(i + 1)
  return {
    normalized,
    base,
    variant: rest || null,
    digits: normalized.replace(/^\p{L}+/u, ''),
  }
}
