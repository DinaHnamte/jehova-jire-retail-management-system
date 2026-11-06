// Copy for delivery statuses and failure reasons (Phase 6a Global Constraints).
import { formatRupees, toPaise } from '@/shared/money'

export type DeliveryStatus = 'to_send' | 'assigned' | 'picked_up' | 'delivered' | 'failed' | 'returned' | 'cancelled'
export type FailReason = 'customer_unavailable' | 'address_not_found' | 'customer_refused' | 'unreachable_by_phone' | 'other'

const statuses: Record<DeliveryStatus, string> = {
  to_send: 'To send',
  assigned: 'Assigned',
  picked_up: 'Out for delivery',
  delivered: 'Delivered',
  failed: "Couldn't deliver",
  returned: 'Returned to shop',
  cancelled: 'Cancelled',
}

/** Failed reasons in the order they are offered. */
export const reasons: { code: FailReason; label: string }[] = [
  { code: 'customer_unavailable', label: 'Customer not there' },
  { code: 'address_not_found', label: "Couldn't find the address" },
  { code: 'customer_refused', label: 'Customer refused' },
  { code: 'unreachable_by_phone', label: "Customer didn't answer the phone" },
  { code: 'other', label: 'Other' },
]

export const statusLabel = (code: string): string => (statuses as Record<string, string>)[code] ?? code
export const reasonLabel = (code: string): string => reasons.find((r) => r.code === code)?.label ?? code

export type DeliveryAction =
  | 'assign'
  | 'reassign'
  | 'edit'
  | 'cancel'
  | 'picked_up'
  | 'delivered'
  | 'failed'
  | 'retry'
  | 'check_in'

/** What the owner can do to a delivery in this status (spec §1). */
export function actionsFor(status: DeliveryStatus, hasDriver: boolean): DeliveryAction[] {
  switch (status) {
    case 'to_send':
      return ['assign', 'edit', 'cancel']
    case 'assigned':
      return [hasDriver ? 'reassign' : 'assign', 'edit', 'cancel', 'picked_up']
    case 'picked_up':
      return ['delivered', 'failed']
    case 'failed':
      return ['retry', 'check_in']
    default:
      return []
  }
}

/** Time since `iso`: "now", "10 min", "2 h", "3 d". */
export function ageLabel(iso: string, now: Date = new Date()): string {
  const mins = Math.max(0, Math.floor((now.getTime() - new Date(iso).getTime()) / 60000))
  if (mins < 1) return 'now'
  if (mins < 60) return `${mins} min`
  if (mins < 60 * 24) return `${Math.floor(mins / 60)} h`
  return `${Math.floor(mins / (60 * 24))} d`
}

/** Why a sale with a delivery in this status can't be cancelled (Ruling 4); null when it can. */
export function saleCancelNote(status: string): string | null {
  switch (status) {
    case 'picked_up':
      return "This delivery is out with the driver. Mark it delivered or couldn't deliver first."
    case 'failed':
      return "This delivery couldn't be delivered. Check the goods back in to cancel it."
    case 'delivered':
      return 'This delivery is done. Return items instead.'
    default:
      return null
  }
}

/** The owner's money line for a delivery: who collects what is still owed, or "Paid". */
export function moneyLabel(d: { collector: 'shop' | 'driver'; owed: string }): string {
  if (toPaise(d.owed) <= 0n) return 'Paid'
  return d.collector === 'shop' ? `Shop collects ${formatRupees(d.owed)}` : `Collect ${formatRupees(d.owed)}`
}
