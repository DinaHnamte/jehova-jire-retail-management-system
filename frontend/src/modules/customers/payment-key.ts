import { persistedKeys } from '@/shared/idempotency'
import type { CustomerPaymentBody } from '@/modules/pos/api'

const keys = persistedKeys('jj.payment.v1')

/**
 * Keys for one customer's later payments. The key follows the exact body (method, amount,
 * note) and is kept in localStorage until a payment succeeds or the server rejects it, not
 * just while the dialog is open: after a lost response the same payment, retried even after
 * closing the dialog or reloading the page, reuses its key, so the server records it once.
 * `pending()` is the last body tried and not known to have been answered.
 */
export function makePaymentSession(customerId: string) {
  return {
    key(body: CustomerPaymentBody): string {
      return keys.key(customerId, body)
    },
    pending(): CustomerPaymentBody | null {
      const raw = keys.pendingBody(customerId)
      if (raw === null) return null
      try {
        return JSON.parse(raw) as CustomerPaymentBody
      } catch {
        return null
      }
    },
    /** The server answered (paid, or rejected the body): the next try is a new payment with a new key. */
    succeeded() {
      keys.clear(customerId)
    },
  }
}
