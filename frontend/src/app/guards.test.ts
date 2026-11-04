import { describe, expect, it } from 'vitest'
import { authGuard } from './guards'

const route = (path: string, meta: { perm?: string; public?: boolean; guestOnly?: boolean } = {}) => ({
  fullPath: path,
  meta,
})
const anon = { isLoggedIn: false, can: () => false }
const cashier = { isLoggedIn: true, can: (p: string) => p === 'orders.create' }

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
})
