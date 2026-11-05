export interface NavItem {
  label: string
  to: string
  perm?: string
  /** Icon name, see NavIcon.vue. */
  icon: NavIconName
}

export type NavIconName = 'overview' | 'products' | 'users'

/** Main menu. Later phases append items (POS, Products, …). */
export const navItems: NavItem[] = [
  { label: 'Overview', to: '/', icon: 'overview' },
  { label: 'Products', to: '/products', perm: 'products.read', icon: 'products' },
  { label: 'Users', to: '/admin/users', perm: 'users.manage', icon: 'users' },
]

export function visibleNav(can: (perm: string) => boolean): NavItem[] {
  return navItems.filter((i) => !i.perm || can(i.perm))
}
