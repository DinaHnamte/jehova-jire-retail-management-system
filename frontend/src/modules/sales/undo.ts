// Idempotency keys for cancelling a sale or taking a return, kept in localStorage so a retry
// after a lost response, or after a reload, sends the same key with the same body.
import { persistedKeys } from '@/shared/idempotency'
import type { Disposition, PayMethod, ReturnBody } from '@/modules/pos/api'

const keys = persistedKeys('jj.undo.v1')

/** The key for `body` under `scope`: unchanged for the same body, new for a different one. */
export function pendingUndoKey(scope: string, body: unknown): string {
  return keys.key(scope, body)
}

/** Forget the key once the request has succeeded. */
export function clearUndoKey(scope: string) {
  keys.clear(scope)
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
