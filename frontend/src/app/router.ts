import { createRouter, createWebHistory } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { authGuard } from './guards'
import { clearReloadFlag, isChunkLoadError, reloadOnce } from './chunk-reload'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      component: () => import('@/modules/auth/LoginPage.vue'),
      meta: { public: true, guestOnly: true, title: 'Sign in' },
    },
    {
      path: '/',
      component: () => import('./layouts/AppLayout.vue'),
      children: [
        { path: '', component: () => import('@/modules/overview/OverviewPage.vue'), meta: { title: 'Overview' } },
        {
          path: 'admin/users',
          component: () => import('@/modules/admin/UsersPage.vue'),
          meta: { perm: 'users.manage', title: 'Users' },
        },
        { path: 'forbidden', component: () => import('./ForbiddenPage.vue'), meta: { title: 'Not allowed' } },
        { path: ':pathMatch(.*)*', component: () => import('./NotFoundPage.vue'), meta: { title: 'Not found' } },
      ],
    },
  ],
})

router.beforeEach(async (to) => {
  const session = useSessionStore()
  if (!session.loaded) await session.load().catch(() => {})
  return authGuard(to, { isLoggedIn: session.isLoggedIn, can: (p) => session.can(p) })
})

router.onError((err, to) => {
  if (isChunkLoadError(err)) reloadOnce(to.fullPath)
})

router.afterEach((to) => {
  clearReloadFlag()
  document.title = to.meta.title ? `${String(to.meta.title)} · Jehovah Jireh` : 'Jehovah Jireh'
})
