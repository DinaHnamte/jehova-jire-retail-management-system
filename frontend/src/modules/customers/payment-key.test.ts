import { describe, expect, it } from 'vitest'
import { makePaymentSession } from './payment-key'

const body = (amount: string, method: 'cash' | 'upi' = 'cash') => ({ method, amount, note: 'x' })

describe('payment key', () => {
  it('same body, same key', () => {
    const s = makePaymentSession()
    expect(s.key(body('200.00'))).toBe(s.key(body('200.00')))
  })
  it('changed amount or method gives a new key', () => {
    const s = makePaymentSession()
    const a = s.key(body('200.00'))
    const b = s.key(body('150.00'))
    const c = s.key(body('150.00', 'upi'))
    expect(new Set([a, b, c]).size).toBe(3)
  })
  it('reverting does not bring back the old key', () => {
    const s = makePaymentSession()
    const a = s.key(body('200.00'))
    s.key(body('150.00'))
    expect(s.key(body('200.00'))).not.toBe(a)
  })
  it('each customer has its own keys', () => {
    expect(makePaymentSession().key(body('1.00'))).not.toBe(makePaymentSession().key(body('1.00')))
  })
  it('a failed save is remembered until a payment succeeds', () => {
    // Save → network error → dialog closed and opened again → same payment: same key.
    const s = makePaymentSession()
    expect(s.pending()).toBeNull()
    const a = s.key(body('200.00'))
    expect(s.pending()).toEqual(body('200.00'))
    expect(s.key(body('200.00'))).toBe(a)
  })
  it('after a success the next payment gets a new key', () => {
    const s = makePaymentSession()
    const a = s.key(body('200.00'))
    s.succeeded()
    expect(s.pending()).toBeNull()
    expect(s.key(body('200.00'))).not.toBe(a)
  })
})
