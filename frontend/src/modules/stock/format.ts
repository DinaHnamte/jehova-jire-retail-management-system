import { formatRupees } from '@/shared/money'
import type { MovementType } from './api'

const TZ = 'Asia/Kolkata'

const LABELS: Record<MovementType, string> = {
  opening_balance: 'Opening count',
  purchase_receipt: 'Received',
  adjustment_in: 'Added',
  adjustment_out: 'Removed',
  damage: 'Damaged',
}

export const movementLabel = (t: MovementType) => LABELS[t]

/** `+5`, `−2` (true minus sign), `0`. */
export const signed = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '0')

/** Average cost (4 decimals) shown as rupees to the paisa, rounding half up. */
export function formatCost(avg: string | null): string {
  if (avg === null) return '—'
  const [int = '0', frac = ''] = avg.split('.')
  const f = frac.padEnd(4, '0')
  let paise = BigInt(int) * 100n + BigInt(f.slice(0, 2))
  if (Number(f[2]) >= 5) paise += 1n
  const s = paise.toString().padStart(3, '0')
  return formatRupees(`${s.slice(0, -2)}.${s.slice(-2)}`)
}

/** `2026-11-06` → `6 Nov 2026`. */
export function formatDay(date: string): string {
  const [y, m, d] = date.split('-').map(Number)
  return new Intl.DateTimeFormat('en-IN', { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' }).format(
    new Date(Date.UTC(y!, m! - 1, d!)),
  )
}

/** A timestamp in shop time, e.g. `7 Nov 2026, 2:00 am`. */
export function formatWhen(iso: string): string {
  return new Intl.DateTimeFormat('en-IN', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZone: TZ,
  })
    .format(new Date(iso))
    .replace(/\b(AM|PM)\b/, (m) => m.toLowerCase())
}
