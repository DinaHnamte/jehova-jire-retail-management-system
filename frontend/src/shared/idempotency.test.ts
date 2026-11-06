import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { PENDING_TTL_MS, attemptSession, makeKeyed } from './idempotency'

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

function fakeStorage() {
  const m = new Map<string, string>()
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  }
}

describe('attemptSession', () => {
  const SK = 'jj.test.v1'
  beforeEach(() => {
    vi.stubGlobal('localStorage', fakeStorage())
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-11-06T10:00:00Z'))
  })
  afterEach(() => vi.useRealTimers())

  /** An attempt whose response was lost: a key was issued and nothing answered it. */
  function lost(scope: string, body: unknown): string {
    const s = attemptSession(SK, scope)
    expect(s.open()).toBeNull()
    return s.key(body)
  }

  it('no pending attempt: no choice, key can be issued', () => {
    const s = attemptSession(SK, 'c1')
    expect(s.open()).toBeNull()
    expect(s.needsChoice()).toBe(false)
    expect(s.key({ a: 1 })).toMatch(/[0-9a-f-]{36}/)
  })

  it('pending attempt present: a choice is required before any key', () => {
    lost('c1', { a: 500 })
    const s = attemptSession(SK, 'c1')
    expect(s.open()).toEqual({ a: 500 })
    expect(s.needsChoice()).toBe(true)
    // Even the identical body never silently gets the old key.
    expect(() => s.key({ a: 500 })).toThrow()
  })

  it('retry gives back the pending body and its same key', () => {
    const k = lost('c1', { a: 500 })
    const s = attemptSession(SK, 'c1')
    s.open()
    expect(s.retry()).toEqual({ a: 500 })
    expect(s.needsChoice()).toBe(false)
    expect(s.key({ a: 500 })).toBe(k)
  })

  it('start new clears the entry and the same body gets a new key', () => {
    const k = lost('c1', { a: 500 })
    const s = attemptSession(SK, 'c1')
    s.open()
    s.startNew()
    expect(s.needsChoice()).toBe(false)
    expect(attemptSession(SK, 'c1').open()).toBeNull()
    const s2 = attemptSession(SK, 'c1')
    s2.open()
    expect(s.key({ a: 500 })).not.toBe(k)
  })

  it('a retry in the same opening reuses the key (no connection, press again)', () => {
    const s = attemptSession(SK, 'c1')
    s.open()
    const k = s.key({ a: 1 })
    expect(s.key({ a: 1 })).toBe(k)
  })

  it('done clears only that scope', () => {
    lost('c1', { a: 1 })
    lost('c2', { a: 2 })
    const s = attemptSession(SK, 'c1')
    s.open()
    s.retry()
    s.done()
    expect(attemptSession(SK, 'c1').open()).toBeNull()
    expect(attemptSession(SK, 'c2').open()).toEqual({ a: 2 })
  })

  it('entries older than 24 h are ignored and pruned', () => {
    lost('old', { a: 1 })
    vi.setSystemTime(Date.now() + PENDING_TTL_MS - 1000)
    lost('fresh', { a: 2 })
    vi.setSystemTime(Date.now() + 2000)
    expect(attemptSession(SK, 'old').open()).toBeNull()
    expect(attemptSession(SK, 'fresh').open()).toEqual({ a: 2 })
    const raw = JSON.parse(localStorage.getItem(SK) ?? '{}') as Record<string, unknown>
    expect(Object.keys(raw)).toEqual(['fresh'])
  })

  it('a retried key keeps its first issue time', () => {
    lost('c1', { a: 1 })
    vi.setSystemTime(Date.now() + PENDING_TTL_MS - 1000)
    const s = attemptSession(SK, 'c1')
    s.open()
    s.retry()
    s.key({ a: 1 })
    vi.setSystemTime(Date.now() + 2000)
    expect(attemptSession(SK, 'c1').open()).toBeNull()
  })

  it('an entry saved before issue times were kept still asks for a choice', () => {
    localStorage.setItem(SK, JSON.stringify({ c1: { body: '{"a":1}', value: 'k-old' } }))
    const s = attemptSession(SK, 'c1')
    expect(s.open()).toEqual({ a: 1 })
    expect(s.needsChoice()).toBe(true)
  })

  it('works when storage throws', () => {
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
    const s = attemptSession(SK, 'c1')
    expect(s.open()).toBeNull()
    expect(s.key({ a: 1 })).toMatch(/[0-9a-f-]{36}/)
    expect(() => s.done()).not.toThrow()
  })
})
