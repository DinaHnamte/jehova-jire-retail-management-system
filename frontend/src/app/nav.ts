export interface NavItem {
  label: string
  to: string
  perm?: string
  /** Icon name, see NavIcon.vue. */
  icon: NavIconName
}

export type NavIconName = 'overview' | 'pos' | 'sales' | 'deliveries' | 'customers' | 'products' | 'stock' | 'users'

/** Main menu. Later phases append items (POS, Products, …). */
export const navItems: NavItem[] = [
  { label: 'Overview', to: '/', icon: 'overview' },
  { label: 'Sell', to: '/pos', perm: 'orders.confirm', icon: 'pos' },
  { label: 'Sales', to: '/sales', perm: 'orders.read', icon: 'sales' },
  { label: 'Deliveries', to: '/deliveries', perm: 'deliveries.read', icon: 'deliveries' },
  { label: 'Customers', to: '/customers', perm: 'orders.read', icon: 'customers' },
  { label: 'Products', to: '/products', perm: 'products.read', icon: 'products' },
  { label: 'Stock', to: '/stock', perm: 'inventory.read', icon: 'stock' },
  { label: 'Users', to: '/admin/users', perm: 'users.manage', icon: 'users' },
]

export function visibleNav(can: (perm: string) => boolean): NavItem[] {
  return navItems.filter((i) => !i.perm || can(i.perm))
}

/**
 * Phone tab bar: all items when they fit in `max + 1` slots, otherwise the first `max` as tabs
 * and the rest behind a "More" tab.
 */
export function splitForTabBar<T>(items: T[], max = 4): { tabs: T[]; more: T[] } {
  if (items.length <= max + 1) return { tabs: items, more: [] }
  return { tabs: items.slice(0, max), more: items.slice(max) }
}
