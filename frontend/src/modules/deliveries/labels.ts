// Copy for delivery statuses and failure reasons (Phase 6a Global Constraints).

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
