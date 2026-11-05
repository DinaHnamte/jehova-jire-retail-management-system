import { describe, expect, it } from 'vitest'
import { makeKeyed } from './idempotency'

describe('makeKeyed', () => {
  it('same body gives the same key', () => {
    const k = makeKeyed()
    expect(k.key({ a: 1 })).toBe(k.key({ a: 1 }))
  })
  it('a changed body gives a new key', () => {
    const k = makeKeyed()
    expect(k.key({ a: 1 })).not.toBe(k.key({ a: 2 }))
  })
  it('a reverted body gets a new key, never the old one', () => {
    const k = makeKeyed()
    const k1 = k.key({ a: 1 })
    const k2 = k.key({ a: 2 })
    const k3 = k.key({ a: 1 })
    expect(k3).not.toBe(k1)
    expect(k3).not.toBe(k2)
  })
})
