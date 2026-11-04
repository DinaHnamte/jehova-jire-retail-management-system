export interface GuardRoute {
  fullPath: string
  meta: { perm?: string; public?: boolean; guestOnly?: boolean }
}

export interface GuardSession {
  isLoggedIn: boolean
  can: (perm: string) => boolean
}

/** Decide whether a navigation may proceed. Pure, so it can be unit-tested. */
export function authGuard(to: GuardRoute, session: GuardSession) {
  if (to.meta.public) {
    return to.meta.guestOnly && session.isLoggedIn ? { path: '/' } : true
  }
  if (!session.isLoggedIn) return { path: '/login', query: { next: to.fullPath } }
  if (to.meta.perm && !session.can(to.meta.perm)) return { path: '/forbidden' }
  return true
}

/** Only allow same-site relative paths as a post-login redirect target. */
export function safeNext(next: unknown): string {
  return typeof next === 'string' && next.startsWith('/') && !next.startsWith('//') ? next : '/'
}
