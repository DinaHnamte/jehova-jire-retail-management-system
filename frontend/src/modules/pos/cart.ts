// Cart arithmetic. Money is a two-decimal string; maths runs in whole paise (BigInt).
import { formatRupees, fromPaise, toPaise } from '@/shared/money'

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

const gross = (l: CartLine): bigint => BigInt(l.quantity) * toPaise(l.unitPrice)

/** quantity × unit price − line discount, or null when the discount is more than the line. */
export function lineNet(l: CartLine): string | null {
  const net = gross(l) - toPaise(l.lineDiscount)
  return net < 0n ? null : fromPaise(net)
}

export interface CartTotals {
  subtotal: string
  discount: string
  total: string
  /** The order discount is more than the subtotal; the server would reject this cart. */
  overDiscount: boolean
}

/**
 * Nothing is capped silently. An over-discounted line counts as zero toward the subtotal and an
 * over-large order discount sets `overDiscount` (the total then reads 0.00); callers must treat
 * the cart as invalid in either case (see `cartProblems`).
 */
export function cartTotals(lines: CartLine[], orderDiscount: string): CartTotals {
  const subtotal = lines.reduce((sum, l) => {
    const n = gross(l) - toPaise(l.lineDiscount)
    return sum + (n < 0n ? 0n : n)
  }, 0n)
  const discount = toPaise(orderDiscount)
  const over = discount > subtotal
  return {
    subtotal: fromPaise(subtotal),
    discount: fromPaise(discount),
    total: fromPaise(over ? 0n : subtotal - discount),
    overDiscount: over,
  }
}

export interface CartProblem {
  /** A product id, or `'order'` for the order discount. */
  target: string
  field: 'line_discount' | 'order_discount'
  message: string
}

/** Everything the server would reject for being too big a discount, worded like the server. */
export function cartProblems(lines: CartLine[], orderDiscount: string): CartProblem[] {
  const out: CartProblem[] = []
  for (const l of lines) {
    if (lineNet(l) === null)
      out.push({
        target: l.productId,
        field: 'line_discount',
        message: `Discount is more than this line. Enter ${formatRupees(fromPaise(gross(l)))} or less.`,
      })
  }
  const t = cartTotals(lines, orderDiscount)
  if (t.overDiscount)
    out.push({
      target: 'order',
      field: 'order_discount',
      message: `Discount is more than the bill. Enter ${formatRupees(t.subtotal)} or less.`,
    })
  return out
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
