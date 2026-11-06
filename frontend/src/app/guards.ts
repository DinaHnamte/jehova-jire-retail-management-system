export interface GuardRoute {
  fullPath: string
  meta: { perm?: string; public?: boolean; guestOnly?: boolean; bare?: boolean }
}

export interface GuardSession {
  isLoggedIn: boolean
  /** Has `deliveries.read_own` but not `deliveries.read` (Ruling 11). */
  isDriverOnly: boolean
  can: (perm: string) => boolean
}

/**
 * Bare pages (the customer receipt) are opened by the public on unknown phones. They never need
 * the staff session, so the router must not call /auth/me for them.
 */
export function needsSession(to: Pick<GuardRoute, 'meta'>): boolean {
  return !to.meta.bare
}

/** Where a session lands by default. */
export function homeFor(session: Pick<GuardSession, 'isDriverOnly'>): '/' | '/driver' {
  return session.isDriverOnly ? '/driver' : '/'
}

const isDriverPath = (p: string) => p === '/driver' || p.startsWith('/driver/')

/** Decide whether a navigation may proceed. Pure, so it can be unit-tested. */
export function authGuard(to: GuardRoute, session: GuardSession) {
  if (to.meta.public) {
    return to.meta.guestOnly && session.isLoggedIn ? { path: homeFor(session) } : true
  }
  if (!session.isLoggedIn) return { path: '/login', query: { next: to.fullPath } }
  if (session.isDriverOnly && !to.meta.bare && to.fullPath !== '/forbidden' && !isDriverPath(to.fullPath)) {
    return { path: '/driver' }
  }
  if (to.meta.perm && !session.can(to.meta.perm)) return { path: '/forbidden' }
  return true
}

/** Only allow same-site relative paths as a post-login redirect target. */
export function safeNext(next: unknown, fallback = '/'): string {
  return typeof next === 'string' && next.startsWith('/') && !next.startsWith('//') ? next : fallback
}
