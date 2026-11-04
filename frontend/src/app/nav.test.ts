import { describe, expect, it } from 'vitest'
import { visibleNav } from './nav'

describe('visibleNav', () => {
  it('cashier does not see Users', () => {
    const labels = visibleNav((p) => p === 'orders.create').map((i) => i.label)
    expect(labels).toContain('Overview')
    expect(labels).not.toContain('Users')
  })

  it('admin sees Users', () => {
    expect(visibleNav(() => true).map((i) => i.label)).toContain('Users')
  })
})
