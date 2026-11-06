import { describe, expect, it } from 'vitest'
import { authGuard, homeFor, needsSession } from './guards'

const route = (path: string, meta: { perm?: string; public?: boolean; guestOnly?: boolean; bare?: boolean } = {}) => ({
  fullPath: path,
  meta,
})
const anon = { isLoggedIn: false, isDriverOnly: false, can: () => false }
const driver = { isLoggedIn: true, isDriverOnly: true, can: (p: string) => p === 'deliveries.read_own' }
const cashier = { isLoggedIn: true, isDriverOnly: false, can: (p: string) => p === 'orders.create' }

describe('authGuard', () => {
  it('redirects to login on 401 (not logged in)', () => {
    expect(authGuard(route('/users', { perm: 'users.manage' }), anon)).toEqual({
      path: '/login',
      query: { next: '/users' },
    })
  })

  it('sends to forbidden without permission', () => {
    expect(authGuard(route('/users', { perm: 'users.manage' }), cashier)).toEqual({ path: '/forbidden' })
  })

  it('allows logged-in user when no permission required', () => {
    expect(authGuard(route('/'), cashier)).toBe(true)
  })

  it('allows public route when logged out', () => {
    expect(authGuard(route('/login', { public: true }), anon)).toBe(true)
  })

  it('sends logged-in user away from login page', () => {
    expect(authGuard(route('/login', { public: true, guestOnly: true }), cashier)).toEqual({ path: '/' })
  })

  it('lets an anonymous visitor open the public receipt without redirecting', () => {
    expect(authGuard(route('/r/abc', { public: true, bare: true }), anon)).toBe(true)
  })

  it('does not need a session for bare pages, but does for app pages', () => {
    expect(needsSession(route('/r/abc', { public: true, bare: true }))).toBe(false)
    expect(needsSession(route('/login', { public: true, guestOnly: true }))).toBe(true)
    expect(needsSession(route('/sales'))).toBe(true)
  })

  describe('driver-only sessions', () => {
    it('sends / and /sales to /driver', () => {
      expect(authGuard(route('/'), driver)).toEqual({ path: '/driver' })
      expect(authGuard(route('/sales', { perm: 'orders.read' }), driver)).toEqual({ path: '/driver' })
    })

    it('allows /driver pages, bare pages and forbidden', () => {
      expect(authGuard(route('/driver/deliveries/x', { perm: 'deliveries.read_own' }), driver)).toBe(true)
      expect(authGuard(route('/r/abc', { public: true, bare: true }), driver)).toBe(true)
      expect(authGuard(route('/forbidden'), driver)).toBe(true)
    })

    it('sends a driver away from the login page to /driver', () => {
      expect(authGuard(route('/login', { public: true, guestOnly: true }), driver)).toEqual({ path: '/driver' })
    })

    it('lets an admin open /driver', () => {
      const admin = { isLoggedIn: true, isDriverOnly: false, can: () => true }
      expect(authGuard(route('/driver', { perm: 'deliveries.read_own' }), admin)).toBe(true)
    })
  })

  it('homeFor picks the driver home for driver-only sessions', () => {
    expect(homeFor(driver)).toBe('/driver')
    expect(homeFor(cashier)).toBe('/')
  })
})
