import { makeKeyed } from '@/shared/idempotency'
import type { CustomerPaymentBody } from '@/modules/pos/api'

/** One per dialog instance: the key follows the exact body (method, amount, note). */
export function makePaymentKey() {
  const keyed = makeKeyed()
  return (body: CustomerPaymentBody): string => keyed.key(body)
}
