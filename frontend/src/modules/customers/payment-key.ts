import { makeKeyed } from '@/shared/idempotency'
import type { CustomerPaymentBody } from '@/modules/pos/api'

/**
 * Keys for one customer's later payments. The key follows the exact body (method, amount,
 * note) and lives until a payment succeeds, not just while the dialog is open: after a lost
 * response the same payment, retried even after closing and reopening, reuses its key, so
 * the server records it once. `pending()` is the last body tried and not known to succeed.
 */
export function makePaymentSession() {
  let keyed = makeKeyed()
  let pending: CustomerPaymentBody | null = null
  return {
    key(body: CustomerPaymentBody): string {
      pending = body
      return keyed.key(body)
    },
    pending: (): CustomerPaymentBody | null => pending,
    /** The payment went through: the next one is a new payment with a new key. */
    succeeded() {
      keyed = makeKeyed()
      pending = null
    },
  }
}
