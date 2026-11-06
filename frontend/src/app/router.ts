import { createRouter, createWebHistory } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { authGuard, needsSession } from './guards'
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
      path: '/r/:token',
      component: () => import('@/modules/receipt/PublicReceiptPage.vue'),
      meta: { public: true, bare: true, title: 'Receipt' },
    },
    {
      path: '/',
      component: () => import('./layouts/AppLayout.vue'),
      children: [
        { path: '', component: () => import('@/modules/overview/OverviewPage.vue'), meta: { title: 'Overview' } },
        {
          path: 'pos',
          component: () => import('@/modules/pos/PosPage.vue'),
          meta: { perm: 'orders.confirm', title: 'Sell' },
        },
        {
          path: 'sales',
          component: () => import('@/modules/sales/SalesPage.vue'),
          meta: { perm: 'orders.read', title: 'Sales' },
        },
        {
          path: 'sales/:id',
          component: () => import('@/modules/sales/SaleDetailPage.vue'),
          meta: { perm: 'orders.read', title: 'Sale' },
        },
        {
          path: 'customers',
          component: () => import('@/modules/customers/CustomersPage.vue'),
          meta: { perm: 'orders.read', title: 'Customers' },
        },
        {
          path: 'customers/:id',
          component: () => import('@/modules/customers/CustomerDetailPage.vue'),
          meta: { perm: 'orders.read', title: 'Customer' },
        },
        {
          path: 'products',
          component: () => import('@/modules/products/ProductsPage.vue'),
          meta: { perm: 'products.read', title: 'Products' },
        },
        {
          path: 'products/new',
          component: () => import('@/modules/products/ProductFormPage.vue'),
          meta: { perm: 'products.create', title: 'Add product' },
        },
        {
          path: 'products/import',
          component: () => import('@/modules/products/ImportPage.vue'),
          meta: { perm: 'products.create', title: 'Import products' },
        },
        {
          path: 'products/:id',
          component: () => import('@/modules/products/ProductDetailPage.vue'),
          meta: { perm: 'products.read', title: 'Product' },
        },
        {
          path: 'products/:id/edit',
          component: () => import('@/modules/products/ProductFormPage.vue'),
          meta: { perm: 'products.update', title: 'Edit product' },
        },
        {
          path: 'stock',
          component: () => import('@/modules/stock/StockPage.vue'),
          meta: { perm: 'inventory.read', title: 'Stock' },
        },
        {
          path: 'stock/suppliers',
          component: () => import('@/modules/stock/SuppliersPage.vue'),
          meta: { perm: 'purchases.read', title: 'Suppliers' },
        },
        {
          path: 'stock/receipts',
          component: () => import('@/modules/stock/ReceiptsPage.vue'),
          meta: { perm: 'purchases.read', title: 'Receive goods' },
        },
        {
          path: 'stock/receipts/:id',
          component: () => import('@/modules/stock/ReceiptEditorPage.vue'),
          meta: { perm: 'purchases.read', title: 'Receipt' },
        },
        {
          path: 'stock/counts',
          component: () => import('@/modules/stock/CountsPage.vue'),
          meta: { perm: 'inventory.read', title: 'Stock counts' },
        },
        {
          path: 'stock/counts/:id',
          component: () => import('@/modules/stock/CountEditorPage.vue'),
          meta: { perm: 'inventory.read', title: 'Stock count' },
        },
        {
          path: 'lab/ocr',
          component: () => import('@/modules/lab/OcrLabPage.vue'),
          meta: { perm: 'users.manage', title: 'OCR test' },
        },
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
  if (needsSession(to) && !session.loaded) await session.load().catch(() => {})
  return authGuard(to, { isLoggedIn: session.isLoggedIn, can: (p) => session.can(p) })
})

router.onError((err, to) => {
  if (isChunkLoadError(err)) reloadOnce(to.fullPath)
})

router.afterEach((to) => {
  clearReloadFlag()
  document.title = to.meta.title ? `${String(to.meta.title)} · Jehovah Jireh` : 'Jehovah Jireh'
})
