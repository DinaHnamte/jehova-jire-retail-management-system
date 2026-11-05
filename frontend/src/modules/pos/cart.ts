// Cart arithmetic. Money is a two-decimal string; maths runs in whole paise (BigInt).
import { fromPaise, toPaise } from '@/shared/money'

export interface CartLine {
  productId: string
  code: string | null
  name: string
  /** The catalogue selling price when the line was added. */
  listPrice: string
  unitPrice: string
  quantity: number
  /** Total discount on this line (not per unit). */
  lineDiscount: string
}

const max = (a: bigint, b: bigint) => (a > b ? a : b)
const min = (a: bigint, b: bigint) => (a < b ? a : b)

function netPaise(l: CartLine): bigint {
  return max(0n, BigInt(l.quantity) * toPaise(l.unitPrice) - toPaise(l.lineDiscount))
}

/** quantity × unit price − line discount, never below zero. */
export function lineNet(l: CartLine): string {
  return fromPaise(netPaise(l))
}

/** The order discount is capped at the subtotal, so the total is never negative. */
export function cartTotals(lines: CartLine[], orderDiscount: string): { subtotal: string; discount: string; total: string } {
  const subtotal = lines.reduce((sum, l) => sum + netPaise(l), 0n)
  const discount = min(toPaise(orderDiscount), subtotal)
  return { subtotal: fromPaise(subtotal), discount: fromPaise(discount), total: fromPaise(subtotal - discount) }
}

/** `pct` percent of `amount`, rounded half up to the paise. `pct` may have up to two decimals. */
export function percentOff(amount: string, pct: number): string {
  const basis = BigInt(Math.round(pct * 100))
  return fromPaise((toPaise(amount) * basis + 5000n) / 10000n)
}

/** The order discount that brings `total` down to `target`; null when `target` is above `total`. */
export function roundDownTo(total: string, target: string): string | null {
  const t = toPaise(total)
  const g = toPaise(target)
  return g > t ? null : fromPaise(t - g)
}
