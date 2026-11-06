// Idempotency keys for cancelling a sale or taking a return, kept in localStorage so a retry
// after a lost response, or after a reload, sends the same key with the same body. A pending
// attempt is only reused when the user chooses "Retry it".
import { attemptSession, persistedKeys } from '@/shared/idempotency'
import type { CancelBody, Disposition, PayMethod, ReturnBody } from '@/modules/pos/api'

const STORE = 'jj.undo.v1'

export function cancelSession(orderId: string) {
  return attemptSession<CancelBody>(STORE, `cancel:${orderId}`)
}

export function returnSession(orderId: string) {
  return attemptSession<ReturnBody>(STORE, `return:${orderId}`)
}

/** The order is cancelled: nothing more can be cancelled or returned on it, so forget its attempts. */
export function forgetOrderAttempts(orderId: string) {
  const keys = persistedKeys(STORE)
  keys.clear(`cancel:${orderId}`)
  keys.clear(`return:${orderId}`)
}

/** The request body for a return: only lines being returned, a refund method only when one is chosen. */
export function returnBody(
  lines: { order_line_id: string; quantity: number; disposition: Disposition }[],
  method: PayMethod | null,
  note: string,
): ReturnBody {
  const body: ReturnBody = { lines: lines.filter((l) => l.quantity > 0) }
  if (method) body.refund_method = method
  const n = note.trim()
  if (n) body.note = n
  return body
}
