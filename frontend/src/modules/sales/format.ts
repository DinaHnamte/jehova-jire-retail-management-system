import type { PaymentStatus } from '@/modules/pos/api'

const TZ = 'Asia/Kolkata'

/** Today's date in shop time, `YYYY-MM-DD` (what the orders API expects). */
export function todayIst(now: Date = new Date()): string {
  return new Intl.DateTimeFormat('en-CA', { timeZone: TZ, year: 'numeric', month: '2-digit', day: '2-digit' }).format(now)
}

/** `2:05 pm` in shop time. */
export function formatTime(iso: string): string {
  return new Intl.DateTimeFormat('en-IN', { hour: 'numeric', minute: '2-digit', timeZone: TZ })
    .format(new Date(iso))
    .replace(/\b(AM|PM)\b/, (m) => m.toLowerCase())
}

export const fmtPhone = (p: string) => (p.length === 10 ? `${p.slice(0, 5)} ${p.slice(5)}` : p)

export const statusLabel: Record<PaymentStatus, string> = {
  paid: 'Paid',
  partly_paid: 'Partly paid',
  unpaid: 'Unpaid',
  returned: 'Returned',
  cancelled: 'Cancelled',
}
