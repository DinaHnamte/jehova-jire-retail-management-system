<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter, type LocationQueryValue } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import { latestCheck, listStock, runCheck, type CheckRun, type StockFilter, type StockRow } from './api'
import { formatCost, formatWhen } from './format'

const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const canCheck = computed(() => session.can('inventory.adjust'))
const canReceive = computed(() => session.can('purchases.read'))
// Cost is internal: only people who can edit products see it (the API sends null otherwise).
const seesCost = computed(() => session.can('products.update'))

// List state lives in the URL so Back and shared links restore it.
interface Params {
  q: string
  filter: StockFilter | null
  page: number
}
const first = (v: LocationQueryValue | LocationQueryValue[] | undefined) => (Array.isArray(v) ? v[0] : v) ?? null
const params = computed<Params>(() => {
  const f = first(route.query.filter)
  const p = Number(first(route.query.page))
  return {
    q: first(route.query.q) ?? '',
    filter: f === 'out' || f === 'unknown_cost' ? f : null,
    page: Number.isInteger(p) && p >= 1 ? p : 1,
  }
})
function go(next: Partial<Params>, replace = false) {
  const p = { ...params.value, ...next }
  const query: Record<string, string> = {}
  if (p.q) query.q = p.q
  if (p.filter) query.filter = p.filter
  if (p.page > 1) query.page = String(p.page)
  return replace ? router.replace({ query }) : router.push({ query })
}

const page = ref<Page<StockRow> | null>(null)
const check = ref<CheckRun | null>(null)
const error = ref<string | null>(null)
const checkError = ref<string | null>(null)
const loading = ref(false)
const checking = ref(false)
const q = ref(params.value.q)
const searchBox = ref<HTMLInputElement | null>(null)
let debounce: ReturnType<typeof setTimeout> | undefined
let inflight: AbortController | null = null

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
    page.value = await listStock(params.value, ctrl.signal)
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

onMounted(async () => {
  load()
  try {
    check.value = await latestCheck()
  } catch {
    // The banner is a nicety; the list still works without it.
  }
})
onBeforeUnmount(() => {
  clearTimeout(debounce)
  inflight?.abort()
})

async function checkNow() {
  checking.value = true
  checkError.value = null
  try {
    check.value = await runCheck()
  } catch (e) {
    checkError.value = friendlyMessage(e)
  } finally {
    checking.value = false
  }
}

const filters = computed<{ value: StockFilter | null; label: string }[]>(() => [
  { value: null, label: 'All' },
  { value: 'out', label: 'Out of stock' },
  ...(seesCost.value ? [{ value: 'unknown_cost' as const, label: 'Cost unknown' }] : []),
])
const mismatches = computed(() => check.value?.mismatches.length ?? 0)
const lastPage = computed(() => (page.value ? Math.max(1, Math.ceil(page.value.total / page.value.per_page)) : 1))
const range = computed(() => {
  const p = page.value
  if (!p || p.total === 0) return ''
  const from = (p.page - 1) * p.per_page + 1
  const to = Math.min(p.page * p.per_page, p.total)
  return from > p.total ? `${p.total} products` : `${from}–${to} of ${p.total}`
})
</script>

<template>
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-3xl font-extrabold">Stock</h1>
      <p class="mt-1 text-muted">How many of each product the shop has.</p>
    </div>
    <div class="flex flex-wrap gap-2">
      <RouterLink to="/stock/counts" class="btn-secondary">Stock counts</RouterLink>
      <RouterLink v-if="canReceive" to="/stock/receipts" class="btn-primary">Receive goods</RouterLink>
    </div>
  </div>

  <div v-if="mismatches > 0" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 text-danger">
    <p class="font-bold">
      Stock check found {{ mismatches }} {{ mismatches === 1 ? 'mismatch' : 'mismatches' }}. Tell the admin.
    </p>
    <ul class="mt-1 text-sm">
      <li v-for="(m, i) in check!.mismatches" :key="i">
        <template v-if="m.kind === 'stock'">
          {{ m.code ?? m.name }}: app shows {{ m.balance }}, history adds up to {{ m.ledger_sum }}
        </template>
        <template v-else-if="m.kind === 'ledger_unbalanced'">A money entry does not add up to zero.</template>
        <template v-else>An order shows ₹{{ m.owed }} owed, which is not possible.</template>
      </li>
    </ul>
  </div>

  <div class="mt-6">
    <label class="relative block">
      <span class="sr-only">Search stock</span>
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
  </div>

  <div class="mt-4 flex flex-wrap gap-1" role="tablist" aria-label="Show">
    <button
      v-for="f in filters"
      :key="f.label"
      type="button"
      role="tab"
      :aria-selected="params.filter === f.value"
      class="rounded-control px-3.5 py-1.5 font-semibold transition-colors"
      :class="params.filter === f.value ? 'bg-ink text-surface' : 'text-muted hover:bg-ink/5 hover:text-ink'"
      @click="go({ filter: f.value, page: 1 })"
    >
      {{ f.label }}
    </button>
  </div>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <p v-else-if="page && page.items.length === 0" class="mt-6 text-muted">
    <template v-if="params.q">No products match “{{ params.q }}”.</template>
    <template v-else-if="params.filter === 'out'">Nothing is out of stock.</template>
    <template v-else-if="params.filter === 'unknown_cost'">Every product in stock has a cost.</template>
    <template v-else>No products yet. Add products first, then count them.</template>
  </p>

  <div v-else-if="page" class="card mt-6 overflow-hidden" :class="loading ? 'opacity-70' : ''" :aria-busy="loading">
    <table class="w-full text-left">
      <thead class="hidden border-b border-line text-sm text-muted sm:table-header-group">
        <tr>
          <th scope="col" class="px-5 py-3 font-semibold">Product</th>
          <th v-if="seesCost" scope="col" class="px-5 py-3 text-right font-semibold">Average cost</th>
          <th scope="col" class="px-5 py-3 text-right font-semibold">On hand</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="r in page.items"
          :key="r.product.id"
          class="relative grid grid-cols-[1fr_auto] gap-x-4 border-b border-line px-5 py-4 last:border-0 hover:bg-canvas/60 sm:table-row sm:p-0"
        >
          <td class="sm:px-5 sm:py-4">
            <RouterLink :to="`/products/${r.product.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">
              {{ r.product.name }}
            </RouterLink>
            <p class="mt-0.5 flex flex-wrap items-center gap-2 text-sm text-muted">
              <span class="figure text-base font-bold tracking-normal text-ink">{{ r.product.code ?? 'No code' }}</span>
              <Badge v-if="r.product.design_code" tone="clay">{{ r.product.design_code }}</Badge>
            </p>
          </td>
          <td v-if="seesCost" class="col-start-1 row-start-2 text-sm sm:px-5 sm:py-4 sm:text-right sm:text-base">
            <Badge v-if="!r.avg_cost && r.on_hand > 0" tone="golden">Cost unknown</Badge>
            <span v-else class="text-muted">{{ formatCost(r.avg_cost) }}</span>
          </td>
          <td class="col-start-2 row-span-2 row-start-1 self-center text-right sm:px-5 sm:py-4">
            <span class="figure text-2xl" :class="r.on_hand === 0 ? 'text-danger' : ''">{{ r.on_hand }}</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>

  <p v-else class="mt-6 text-muted">Loading stock…</p>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex items-center justify-between gap-3">
    <p class="text-muted">{{ range }}</p>
    <div class="flex gap-2">
      <button type="button" class="btn-secondary" :disabled="params.page <= 1" @click="go({ page: params.page - 1 })">Previous</button>
      <button type="button" class="btn-secondary" :disabled="params.page >= lastPage" @click="go({ page: params.page + 1 })">Next</button>
    </div>
  </nav>
  <p v-else-if="page && page.total > 0" class="mt-4 text-muted">{{ range }}</p>

  <footer class="mt-8 flex flex-wrap items-center gap-3 text-sm text-muted">
    <span v-if="check">Last stock check {{ formatWhen(check.ran_at) }}: {{ mismatches === 0 ? 'all correct' : `${mismatches} to look at` }}.</span>
    <span v-else>The stock check runs every night at 2 am.</span>
    <button v-if="canCheck" type="button" class="link" :disabled="checking" @click="checkNow">
      {{ checking ? 'Checking…' : 'Run check now' }}
    </button>
    <span v-if="checkError" role="alert" class="text-danger">{{ checkError }}</span>
  </footer>
</template>
