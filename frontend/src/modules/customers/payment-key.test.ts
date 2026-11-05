import { describe, expect, it } from 'vitest'
import { makePaymentKey } from './payment-key'

const body = (amount: string, method: 'cash' | 'upi' = 'cash') => ({ method, amount, note: 'x' })

describe('payment key', () => {
  it('same body, same key', () => {
    const k = makePaymentKey()
    expect(k(body('200.00'))).toBe(k(body('200.00')))
  })
  it('changed amount or method gives a new key', () => {
    const k = makePaymentKey()
    const a = k(body('200.00'))
    const b = k(body('150.00'))
    const c = k(body('150.00', 'upi'))
    expect(new Set([a, b, c]).size).toBe(3)
  })
  it('reverting does not bring back the old key', () => {
    const k = makePaymentKey()
    const a = k(body('200.00'))
    k(body('150.00'))
    expect(k(body('200.00'))).not.toBe(a)
  })
  it('each dialog instance has its own keys', () => {
    expect(makePaymentKey()(body('1.00'))).not.toBe(makePaymentKey()(body('1.00')))
  })
})
