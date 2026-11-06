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

describe('undo keys', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', fakeStorage())
  })

  it('gives the same key for the same body', async () => {
    const { pendingUndoKey } = await fresh()
    expect(pendingUndoKey('cancel:1', { reason: 'x' })).toBe(pendingUndoKey('cancel:1', { reason: 'x' }))
  })

  it('keeps the key after a reload', async () => {
    const a = (await fresh()).pendingUndoKey('cancel:1', {})
    const b = (await fresh()).pendingUndoKey('cancel:1', {})
    expect(b).toBe(a)
  })

  it('gives a new key for a changed body', async () => {
    const { pendingUndoKey } = await fresh()
    const a = pendingUndoKey('cancel:1', { reason: 'x' })
    expect(pendingUndoKey('cancel:1', { reason: 'y' })).not.toBe(a)
  })

  it('gives a new key after clear', async () => {
    const { pendingUndoKey, clearUndoKey } = await fresh()
    const a = pendingUndoKey('cancel:1', {})
    clearUndoKey('cancel:1')
    expect(pendingUndoKey('cancel:1', {})).not.toBe(a)
  })

  it('keeps scopes apart', async () => {
    const { pendingUndoKey } = await fresh()
    expect(pendingUndoKey('cancel:1', {})).not.toBe(pendingUndoKey('cancel:2', {}))
  })

  it('works with corrupt storage', async () => {
    localStorage.setItem('jj.undo.v1', '{not json')
    const { pendingUndoKey } = await fresh()
    expect(pendingUndoKey('cancel:1', {})).toMatch(/[0-9a-f-]{36}/)
    localStorage.setItem('jj.undo.v1', '[1,2]')
    expect(pendingUndoKey('cancel:1', {})).toMatch(/[0-9a-f-]{36}/)
  })

  it('works when storage throws', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => { throw new Error('blocked') },
      setItem: () => { throw new Error('blocked') },
      removeItem: () => { throw new Error('blocked') },
    })
    const { pendingUndoKey, clearUndoKey } = await fresh()
    expect(pendingUndoKey('cancel:1', {})).toMatch(/[0-9a-f-]{36}/)
    expect(() => clearUndoKey('cancel:1')).not.toThrow()
  })
})
