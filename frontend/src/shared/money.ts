// Money in the app is a decimal string with two places ("1090.00"), exactly as the API
// sends it. Formatting works on the string so no float rounding can creep in.

const MAX_INT_DIGITS = 7 // NUMERIC(12,2): up to 99,99,999.99

/** `"1090.00"` → `"₹1,090.00"` (Indian grouping); `null` → `"—"`. */
export function formatRupees(amount: string | null | undefined): string {
  if (amount === null || amount === undefined || amount === '') return '—'
  const negative = amount.trim().startsWith('-')
  const [intRaw = '0', fracRaw = ''] = amount.trim().replace(/^-/, '').split('.')
  const int = intRaw.replace(/^0+(?=\d)/, '')
  const frac = (fracRaw + '00').slice(0, 2)
  const last3 = int.slice(-3)
  const head = int.slice(0, -3)
  const grouped = head ? `${head.replace(/\B(?=(\d{2})+$)/g, ',')},${last3}` : last3
  return `${negative ? '-' : ''}₹${grouped}.${frac}`
}

/** What someone typed → `"1090.00"`, or `null` if it isn't a valid amount. */
export function parseMoneyInput(s: string): string | null {
  const t = s.trim().replace(/,/g, '')
  const m = /^(\d+)(?:\.(\d{1,2}))?$/.exec(t)
  if (!m) return null
  const int = m[1]!.replace(/^0+(?=\d)/, '')
  if (int.length > MAX_INT_DIGITS) return null
  return `${int}.${((m[2] ?? '') + '00').slice(0, 2)}`
}

/** `"12.5"` → `1250n`. Expects a non-negative amount with at most two decimals. */
export function toPaise(amount: string): bigint {
  const [int = '0', frac = '00'] = amount.split('.')
  return BigInt(int) * 100n + BigInt(frac.padEnd(2, '0').slice(0, 2))
}

/** `1250n` → `"12.50"`. Negative values are not supported; callers clamp at zero. */
export function fromPaise(p: bigint): string {
  const s = p.toString().padStart(3, '0')
  return `${s.slice(0, -2)}.${s.slice(-2)}`
}
