<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import { listCategories, listProducts, type NamedRef, type Page, type ProductSummary } from './api'
import { fromRouteQuery, toRouteQuery, type ListParams, type Status } from './list-query'

const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const canCreate = computed(() => session.can('products.create'))

const params = computed<ListParams>(() => fromRouteQuery(route.query))
const page = ref<Page<ProductSummary> | null>(null)
const categories = ref<NamedRef[]>([])
const error = ref<string | null>(null)
const loading = ref(false)

// Search box: local text, pushed to the URL after a short pause.
const q = ref(params.value.q)
const searchBox = ref<HTMLInputElement | null>(null)
let debounce: ReturnType<typeof setTimeout> | undefined
let inflight: AbortController | null = null

function go(next: Partial<ListParams>, replace = false) {
  const query = toRouteQuery({ ...params.value, ...next })
  return replace ? router.replace({ query }) : router.push({ query })
}

function onSearchInput() {
  clearTimeout(debounce)
  debounce = setTimeout(() => go({ q: q.value, page: 1 }, true), 120)
}

async function load() {
  inflight?.abort()
  const ctrl = new AbortController()
  inflight = ctrl
  loading.value = true
  error.value = null
  try {
    page.value = await listProducts(params.value, ctrl.signal)
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return
    error.value = friendlyMessage(e)
  } finally {
    if (inflight === ctrl) loading.value = false
  }
}

watch(params, (p) => {
  if (p.q !== q.value && document.activeElement !== searchBox.value) q.value = p.q
  load()
})

// "/" jumps to the search box from anywhere on the page.
function onKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null
  if (e.key === '/' && !(t && (t.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName)))) {
    e.preventDefault()
    searchBox.value?.focus()
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKey)
  load()
  try {
    categories.value = await listCategories()
  } catch {
    // The filter is optional; the list still works without it.
  }
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  clearTimeout(debounce)
  inflight?.abort()
})

const tabs: { value: Status; label: string }[] = [
  { value: 'active', label: 'Active' },
  { value: 'archived', label: 'Archived' },
  { value: 'all', label: 'All' },
]

const range = computed(() => {
  const p = page.value
  if (!p || p.total === 0) return ''
  const from = (p.page - 1) * p.per_page + 1
  const to = Math.min(p.page * p.per_page, p.total)
  return from > p.total ? `${p.total} products` : `${from}–${to} of ${p.total}`
})
const lastPage = computed(() => (page.value ? Math.max(1, Math.ceil(page.value.total / page.value.per_page)) : 1))
const isFiltered = computed(() => !!params.value.q || !!params.value.categoryId || params.value.status !== 'active')
</script>

<template>
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-3xl font-extrabold">Products</h1>
      <p class="mt-1 text-muted">Find a product by its model code or name.</p>
    </div>
    <div v-if="canCreate" class="flex flex-wrap gap-2">
      <RouterLink to="/products/import" class="btn-secondary">Import</RouterLink>
      <RouterLink to="/products/new" class="btn-primary">Add product</RouterLink>
    </div>
  </div>

  <div class="mt-6 grid gap-3 sm:grid-cols-[1fr_auto]">
    <label class="relative block">
      <span class="sr-only">Search products</span>
      <svg viewBox="0 0 24 24" class="pointer-events-none absolute top-1/2 left-3.5 size-5 -translate-y-1/2 text-muted" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" />
      </svg>
      <input
        ref="searchBox"
        v-model="q"
        type="search"
        inputmode="search"
        autocomplete="off"
        autocapitalize="characters"
        spellcheck="false"
        enterkeyhint="search"
        placeholder="Code or name, e.g. 1060 or plate"
        class="field pl-11"
        @input="onSearchInput"
      />
    </label>
    <select
      class="field sm:w-56"
      aria-label="Category"
      :value="params.categoryId ?? ''"
      @change="go({ categoryId: ($event.target as HTMLSelectElement).value || null, page: 1 })"
    >
      <option value="">All categories</option>
      <option v-for="c in categories" :key="c.id" :value="c.id">{{ c.name }}</option>
    </select>
  </div>

  <div class="mt-4 flex gap-1" role="tablist" aria-label="Show">
    <button
      v-for="t in tabs"
      :key="t.value"
      type="button"
      role="tab"
      :aria-selected="params.status === t.value"
      class="rounded-control px-3.5 py-1.5 font-semibold transition-colors"
      :class="params.status === t.value ? 'bg-ink text-surface' : 'text-muted hover:bg-ink/5 hover:text-ink'"
      @click="go({ status: t.value, page: 1 })"
    >
      {{ t.label }}
    </button>
  </div>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <div v-else-if="page && page.total === 0 && !isFiltered" class="card mt-6 px-6 py-10 text-center">
    <h2 class="text-xl font-bold">No products yet</h2>
    <p class="mx-auto mt-1 max-w-prose text-muted">Add products one at a time, or bring them all in from a spreadsheet.</p>
    <div v-if="canCreate" class="mt-5 flex flex-wrap justify-center gap-2">
      <RouterLink to="/products/import" class="btn-secondary">Import from a spreadsheet</RouterLink>
      <RouterLink to="/products/new" class="btn-primary">Add product</RouterLink>
    </div>
  </div>

  <p v-else-if="page && page.items.length === 0" class="mt-6 text-muted">
    <template v-if="params.q">No products match “{{ params.q }}”.</template>
    <template v-else>No products here.</template>
  </p>

  <div v-else-if="page" class="card mt-6 overflow-hidden" :class="loading ? 'opacity-70' : ''" :aria-busy="loading">
    <table class="w-full text-left">
      <thead class="hidden border-b border-line text-sm text-muted sm:table-header-group">
        <tr>
          <th scope="col" class="px-5 py-3 font-semibold">Product</th>
          <th scope="col" class="px-5 py-3 font-semibold">Category</th>
          <th scope="col" class="px-5 py-3 text-right font-semibold">Price</th>
          <th scope="col" class="px-5 py-3 text-right font-semibold">MRP</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="p in page.items"
          :key="p.id"
          class="relative grid grid-cols-[1fr_auto] gap-x-4 border-b border-line px-5 py-4 last:border-0 hover:bg-canvas/60 sm:table-row sm:p-0"
        >
          <td class="sm:px-5 sm:py-4">
            <RouterLink :to="`/products/${p.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">
              {{ p.name }}
            </RouterLink>
            <p class="mt-0.5 flex flex-wrap items-center gap-2 text-sm text-muted">
              <span class="figure text-base font-bold tracking-normal text-ink">{{ p.code ?? 'No code' }}</span>
              <Badge v-if="p.archived" tone="neutral">Archived</Badge>
            </p>
          </td>
          <td class="col-start-1 row-start-2 text-sm text-muted sm:px-5 sm:py-4 sm:text-base">
            {{ p.category?.name ?? '' }}
          </td>
          <td class="row-span-2 self-center text-right sm:px-5 sm:py-4">
            <span class="figure text-lg">{{ formatRupees(p.selling_price) }}</span>
            <span v-if="p.mrp" class="block text-sm text-muted sm:hidden">MRP {{ formatRupees(p.mrp) }}</span>
          </td>
          <td class="hidden text-right text-muted sm:table-cell sm:px-5 sm:py-4">{{ formatRupees(p.mrp) }}</td>
        </tr>
      </tbody>
    </table>
  </div>

  <p v-else class="mt-6 text-muted">Loading products…</p>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex items-center justify-between gap-3">
    <p class="text-muted">{{ range }}</p>
    <div class="flex gap-2">
      <button type="button" class="btn-secondary" :disabled="params.page <= 1" @click="go({ page: params.page - 1 })">
        Previous
      </button>
      <button type="button" class="btn-secondary" :disabled="params.page >= lastPage" @click="go({ page: params.page + 1 })">
        Next
      </button>
    </div>
  </nav>
  <p v-else-if="page && page.total > 0" class="mt-4 text-muted">{{ range }}</p>
</template>
