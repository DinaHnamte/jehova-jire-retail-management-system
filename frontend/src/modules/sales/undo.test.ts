import { beforeEach, describe, expect, it, vi } from 'vitest'

function fakeStorage() {
  const m = new Map<string, string>()
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  }
}

async function fresh() {
  vi.resetModules()
  return import('./undo')
}

describe('returnBody', () => {
  const lines = [
    { order_line_id: 'a', quantity: 0, disposition: 'sellable' as const },
    { order_line_id: 'b', quantity: 2, disposition: 'damaged' as const },
  ]

  it('drops quantity-0 lines and omits a null refund method', async () => {
    const { returnBody } = await fresh()
    expect(returnBody(lines, null, '')).toEqual({ lines: [lines[1]] })
  })

  it('includes the refund method and a trimmed note when given', async () => {
    const { returnBody } = await fresh()
    expect(returnBody(lines, 'upi', '  torn  ')).toEqual({ lines: [lines[1]], refund_method: 'upi', note: 'torn' })
  })
})

describe('undo sessions', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', fakeStorage())
  })

  it('a pending cancel survives a reload and must be chosen', async () => {
    const { cancelSession } = await fresh()
    const s = cancelSession('1')
    expect(s.open()).toBeNull()
    const k = s.key({ reason: 'x' })
    const again = (await fresh()).cancelSession('1')
    expect(again.open()).toEqual({ reason: 'x' })
    expect(again.needsChoice()).toBe(true)
    expect(again.retry()).toEqual({ reason: 'x' })
    expect(again.key({ reason: 'x' })).toBe(k)
  })

  it('start again gives an identical return a new key', async () => {
    const { returnSession } = await fresh()
    const body = { lines: [{ order_line_id: 'a', quantity: 1, disposition: 'sellable' as const }] }
    const s = returnSession('1')
    s.open()
    const k = s.key(body)
    const again = returnSession('1')
    expect(again.open()).toEqual(body)
    again.startNew()
    expect(again.key(body)).not.toBe(k)
  })

  it('keeps cancel and return and orders apart', async () => {
    const { cancelSession, returnSession } = await fresh()
    const a = cancelSession('1')
    a.open()
    a.key({})
    expect(returnSession('1').open()).toBeNull()
    expect(cancelSession('2').open()).toBeNull()
  })

  it('forgetOrderAttempts clears both scopes of that order only', async () => {
    const { cancelSession, returnSession, forgetOrderAttempts } = await fresh()
    for (const s of [cancelSession('1'), returnSession('1'), cancelSession('2')]) {
      s.open()
      s.key({ lines: [] })
    }
    forgetOrderAttempts('1')
    expect(cancelSession('1').open()).toBeNull()
    expect(returnSession('1').open()).toBeNull()
    expect(cancelSession('2').open()).not.toBeNull()
  })

  it('works with corrupt storage', async () => {
    localStorage.setItem('jj.undo.v1', '{not json')
    const { cancelSession } = await fresh()
    const s = cancelSession('1')
    expect(s.open()).toBeNull()
    expect(s.key({})).toMatch(/[0-9a-f-]{36}/)
    localStorage.setItem('jj.undo.v1', '[1,2]')
    expect(cancelSession('1').open()).toBeNull()
  })
})
