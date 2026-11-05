// Receipt and count arithmetic. Money is a two-decimal string; maths runs in whole paise
// (BigInt) so no float rounding can creep in.
import { parseMoneyInput } from '@/shared/money'

const MAX_QTY = 1_000_000

/** A whole quantity from 1 to 1,000,000, or null. */
export function parseQuantity(s: string, min = 1): number | null {
  const t = s.trim()
  if (!/^\d+$/.test(t)) return null
  const n = Number(t)
  return n >= min && n <= MAX_QTY ? n : null
}

function toPaise(amount: string): bigint {
  const [int = '0', frac = '00'] = amount.split('.')
  return BigInt(int) * 100n + BigInt(frac.padEnd(2, '0').slice(0, 2))
}

function fromPaise(p: bigint): string {
  const s = p.toString().padStart(3, '0')
  return `${s.slice(0, -2)}.${s.slice(-2)}`
}

/** quantity × cost as `"1205.00"`, or null when either is invalid. */
export function lineTotal(quantity: string, cost: string): string | null {
  const q = parseQuantity(quantity)
  const c = parseMoneyInput(cost)
  if (q === null || c === null) return null
  return fromPaise(BigInt(q) * toPaise(c))
}

/** Sum of the valid lines; invalid lines count as zero. */
export function grandTotal(lines: { quantity: string; unit_cost: string }[]): string {
  let total = 0n
  for (const l of lines) {
    const t = lineTotal(l.quantity, l.unit_cost)
    if (t !== null) total += toPaise(t)
  }
  return fromPaise(total)
}

/** counted − on hand, or null while the count box is empty or invalid. */
export function countDifference(counted: string, onHand: number): number | null {
  const n = parseQuantity(counted, 0)
  return n === null ? null : n - onHand
}

/** `"lines[2].unit_cost"` → `{ index: 2, key: 'unit_cost' }`, for pointing an API error at a row. */
export function fieldToLineIndex(field: string | undefined | null): { index: number; key: string } | null {
  const m = /^lines\[(\d+)\]\.(\w+)$/.exec(field ?? '')
  return m ? { index: Number(m[1]), key: m[2]! } : null
}
