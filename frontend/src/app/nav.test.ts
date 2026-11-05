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

  it('shows Products to anyone with products.read, after Overview', () => {
    const labels = visibleNav((p) => p === 'products.read').map((i) => i.label)
    expect(labels).toEqual(['Overview', 'Products'])
    expect(visibleNav(() => false).map((i) => i.label)).not.toContain('Products')
  })

  it('shows Stock after Products to anyone with inventory.read', () => {
    const labels = visibleNav((p) => p === 'products.read' || p === 'inventory.read').map((i) => i.label)
    expect(labels).toEqual(['Overview', 'Products', 'Stock'])
    expect(visibleNav((p) => p === 'products.read').map((i) => i.label)).not.toContain('Stock')
  })
})
