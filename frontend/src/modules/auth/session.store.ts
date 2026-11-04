import { defineStore } from 'pinia'
import { api } from '@/shared/api/client'
import { ApiError } from '@/shared/api/errors'

export interface SessionUser {
  id: string
  username: string
  display_name: string
  permissions: string[]
}

interface SessionResponse {
  user: SessionUser
  csrf_token: string
}

export const useSessionStore = defineStore('session', {
  state: () => ({
    user: null as SessionUser | null,
    csrfToken: null as string | null,
    loaded: false,
  }),
  getters: {
    isLoggedIn: (s) => s.user !== null,
  },
  actions: {
    can(perm: string): boolean {
      return this.user?.permissions.includes(perm) ?? false
    },
    set(r: SessionResponse) {
      this.user = r.user
      this.csrfToken = r.csrf_token
    },
    clear() {
      this.user = null
      this.csrfToken = null
    },
    /** Restore the session from the cookie, if any. Safe to call when logged out. */
    async load() {
      try {
        this.set(await api.get<SessionResponse>('/auth/me'))
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 401)) throw e
        this.clear()
      } finally {
        this.loaded = true
      }
    },
    async login(username: string, password: string) {
      this.set(await api.post<SessionResponse>('/auth/login', { username, password }))
    },
    async logout() {
      try {
        await api.post('/auth/logout')
      } finally {
        this.clear()
      }
    },
  },
})
