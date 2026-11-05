export interface NavItem {
  label: string
  to: string
  perm?: string
  /** Icon name, see NavIcon.vue. */
  icon: NavIconName
}

export type NavIconName = 'overview' | 'pos' | 'sales' | 'customers' | 'products' | 'stock' | 'users'

/** Main menu. Later phases append items (POS, Products, …). */
export const navItems: NavItem[] = [
  { label: 'Overview', to: '/', icon: 'overview' },
  { label: 'POS', to: '/pos', perm: 'orders.confirm', icon: 'pos' },
  { label: 'Sales', to: '/sales', perm: 'orders.read', icon: 'sales' },
  { label: 'Customers', to: '/customers', perm: 'orders.read', icon: 'customers' },
  { label: 'Products', to: '/products', perm: 'products.read', icon: 'products' },
  { label: 'Stock', to: '/stock', perm: 'inventory.read', icon: 'stock' },
  { label: 'Users', to: '/admin/users', perm: 'users.manage', icon: 'users' },
]

export function visibleNav(can: (perm: string) => boolean): NavItem[] {
  return navItems.filter((i) => !i.perm || can(i.perm))
}
