import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makePaymentSession } from './payment-key'

function fakeStorage() {
  const m = new Map<string, string>()
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  }
}

const body = (amount: string, method: 'cash' | 'upi' = 'cash') => ({ method, amount, note: 'x' })

describe('payment key', () => {
  beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()))

  it('same body, same key', () => {
    const s = makePaymentSession('c1')
    expect(s.key(body('200.00'))).toBe(s.key(body('200.00')))
  })
  it('changed amount or method gives a new key', () => {
    const s = makePaymentSession('c1')
    const a = s.key(body('200.00'))
    const b = s.key(body('150.00'))
    const c = s.key(body('150.00', 'upi'))
    expect(new Set([a, b, c]).size).toBe(3)
  })
  it('reverting does not bring back the old key', () => {
    const s = makePaymentSession('c1')
    const a = s.key(body('200.00'))
    s.key(body('150.00'))
    expect(s.key(body('200.00'))).not.toBe(a)
  })
  it('each customer has its own keys', () => {
    expect(makePaymentSession('c1').key(body('1.00'))).not.toBe(makePaymentSession('c2').key(body('1.00')))
  })
  it('a failed save is remembered until a payment succeeds', () => {
    // Save → network error → dialog closed and opened again → same payment: same key.
    const s = makePaymentSession('c1')
    expect(s.pending()).toBeNull()
    const a = s.key(body('200.00'))
    expect(s.pending()).toEqual(body('200.00'))
    expect(s.key(body('200.00'))).toBe(a)
  })
  it('after a success the next payment gets a new key', () => {
    const s = makePaymentSession('c1')
    const a = s.key(body('200.00'))
    s.succeeded()
    expect(s.pending()).toBeNull()
    expect(s.key(body('200.00'))).not.toBe(a)
  })
  it('pending_attempt_survives_reload', () => {
    const a = makePaymentSession('c1').key(body('200.00'))
    const reloaded = makePaymentSession('c1')
    expect(reloaded.pending()).toEqual(body('200.00'))
    expect(reloaded.key(body('200.00'))).toBe(a)
    expect(makePaymentSession('c2').pending()).toBeNull()
  })
  it('succeeded clears only that customer', () => {
    makePaymentSession('c1').key(body('200.00'))
    makePaymentSession('c2').key(body('50.00'))
    makePaymentSession('c1').succeeded()
    expect(makePaymentSession('c1').pending()).toBeNull()
    expect(makePaymentSession('c2').pending()).toEqual(body('50.00'))
  })
  it('works without storage', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked')
      },
      setItem: () => {
        throw new Error('blocked')
      },
      removeItem: () => {
        throw new Error('blocked')
      },
    })
    const s = makePaymentSession('c1')
    expect(s.key(body('1.00'))).toBeTruthy()
    expect(s.pending()).toBeNull()
  })
})
