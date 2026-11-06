import { beforeEach, describe, expect, it, vi } from 'vitest'
import { describePayment, makePaymentSession } from './payment-key'

function fakeStorage() {
  const m = new Map<string, string>()
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  }
}

const body = (amount: string, method: 'cash' | 'upi' = 'cash') => ({ method, amount, note: 'x' })

/** Open the dialog with nothing pending and try to pay; the response is lost. */
function lost(customer: string, b = body('500.00')): string {
  const s = makePaymentSession(customer)
  expect(s.open()).toBeNull()
  return s.key(b)
}

describe('payment key', () => {
  beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()))

  it('same body, same key; a changed body gets a new key', () => {
    const s = makePaymentSession('c1')
    s.open()
    const a = s.key(body('200.00'))
    expect(s.key(body('200.00'))).toBe(a)
    expect(s.key(body('150.00'))).not.toBe(a)
    expect(s.key(body('200.00'))).not.toBe(a)
  })

  it('each customer has its own keys', () => {
    expect(lost('c1', body('1.00'))).not.toBe(lost('c2', body('1.00')))
  })

  it('a later identical payment after a lost response must choose first', () => {
    // ₹500 cash sent, response lost, page reloaded. Later another ₹500 cash comes in.
    const k = lost('c1')
    const s = makePaymentSession('c1')
    expect(s.open()).toEqual(body('500.00'))
    expect(s.needsChoice()).toBe(true)
    expect(() => s.key(body('500.00'))).toThrow()
    // Start a new payment: a fresh key, so the server records the second ₹500.
    s.startNew()
    expect(s.key(body('500.00'))).not.toBe(k)
  })

  it('retry it sends the same key', () => {
    const k = lost('c1')
    const s = makePaymentSession('c1')
    s.open()
    expect(s.retry()).toEqual(body('500.00'))
    expect(s.key(body('500.00'))).toBe(k)
  })

  it('done clears only that customer', () => {
    lost('c1')
    lost('c2', body('50.00'))
    makePaymentSession('c1').done()
    expect(makePaymentSession('c1').open()).toBeNull()
    expect(makePaymentSession('c2').open()).toEqual(body('50.00'))
  })

  it('describes the pending payment', () => {
    const r = (s: string) => `₹${s}`
    expect(describePayment(body('500.00'), r)).toBe('₹500.00 (cash)')
    expect(describePayment(body('20.00', 'upi'), r)).toBe('₹20.00 (UPI)')
  })
})
