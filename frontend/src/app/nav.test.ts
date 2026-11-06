import { describe, expect, it } from 'vitest'
import { splitForTabBar, visibleNav } from './nav'

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

  it('shows Sell, Sales and Customers right after Overview for sellers', () => {
    const labels = visibleNav((p) => p === 'orders.confirm' || p === 'orders.read').map((i) => i.label)
    expect(labels).toEqual(['Overview', 'Sell', 'Sales', 'Customers'])
    const stockOnly = visibleNav((p) => p === 'inventory.read').map((i) => i.label)
    expect(stockOnly).not.toContain('Sell')
    expect(stockOnly).not.toContain('Sales')
    expect(stockOnly).not.toContain('Customers')
  })
})

describe('Deliveries nav', () => {
  it('shows Deliveries after Sales with deliveries.read only', () => {
    const labels = visibleNav((p) => p === 'orders.read' || p === 'deliveries.read').map((i) => i.label)
    expect(labels).toEqual(['Overview', 'Sales', 'Deliveries', 'Customers'])
    expect(visibleNav((p) => p === 'deliveries.read_own').map((i) => i.label)).not.toContain('Deliveries')
  })
})

describe('splitForTabBar', () => {
  it('keeps up to five items as tabs', () => {
    expect(splitForTabBar([1, 2, 3, 4, 5])).toEqual({ tabs: [1, 2, 3, 4, 5], more: [] })
    expect(splitForTabBar([1])).toEqual({ tabs: [1], more: [] })
  })
  it('puts everything after the first four behind More when there are six or more', () => {
    expect(splitForTabBar([1, 2, 3, 4, 5, 6, 7])).toEqual({ tabs: [1, 2, 3, 4], more: [5, 6, 7] })
    expect(splitForTabBar([1, 2, 3, 4, 5, 6])).toEqual({ tabs: [1, 2, 3, 4], more: [5, 6] })
  })
})
