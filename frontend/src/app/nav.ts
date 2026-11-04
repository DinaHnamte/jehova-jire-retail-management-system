export interface NavItem {
  label: string
  to: string
  perm?: string
}

/** Main menu. Later phases append items (POS, Products, …). */
export const navItems: NavItem[] = [
  { label: 'Overview', to: '/' },
  { label: 'Users', to: '/admin/users', perm: 'users.manage' },
]

export function visibleNav(can: (perm: string) => boolean): NavItem[] {
  return navItems.filter((i) => !i.perm || can(i.perm))
}
