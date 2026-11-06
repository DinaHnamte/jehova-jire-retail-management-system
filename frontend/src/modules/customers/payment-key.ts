import { attemptSession } from '@/shared/idempotency'
import type { CustomerPaymentBody, PayMethod } from '@/modules/pos/api'

/**
 * One customer's later payments. A payment that got no answer stays pending in localStorage
 * (also across closing the dialog or a reload). When the dialog opens with one pending, the
 * user chooses: retry it (same body, same key, so the server records it once) or start a new
 * payment (new key). A new payment never reuses the old key.
 */
export function makePaymentSession(customerId: string) {
  return attemptSession<CustomerPaymentBody>('jj.payment.v1', customerId)
}

/** "₹500 (cash)" for the pending-payment notice. */
export function describePayment(b: CustomerPaymentBody, rupees: (s: string) => string): string {
  const m: Record<PayMethod, string> = { cash: 'cash', upi: 'UPI' }
  return `${rupees(b.amount)} (${m[b.method]})`
}
