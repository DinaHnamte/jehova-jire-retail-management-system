<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import { listOrders, type OrderSummary } from '@/modules/pos/api'
import { formatTime, todayIst } from './format'
import { formatDay } from '@/modules/stock/format'

const date = ref(todayIst())
const paid = ref<'' | 'paid' | 'partly_paid' | 'unpaid'>('')
const pageNo = ref(1)
const page = ref<Page<OrderSummary> | null>(null)
const error = ref<string | null>(null)
let seq = 0

async function load() {
  const mine = ++seq
  error.value = null
  try {
    const r = await listOrders({
      from: date.value || undefined,
      to: date.value || undefined,
      paid: paid.value || undefined,
      page: pageNo.value,
      per_page: 25,
    })
    if (mine === seq) page.value = r
  } catch (e) {
    if (mine === seq) error.value = friendlyMessage(e)
  }
}
onMounted(load)

function filterChanged() {
  pageNo.value = 1
  load()
}
const lastPage = computed(() => (page.value ? Math.max(1, Math.ceil(page.value.total / page.value.per_page)) : 1))
const owedNow = (o: OrderSummary) => toPaise(o.owed) > 0n
</script>

<template>
  <h1 class="text-3xl font-extrabold">Sales</h1>

  <div class="mt-4 grid grid-cols-2 gap-3 sm:max-w-md">
    <label class="block">
      <span class="text-sm font-semibold">Day</span>
      <input v-model="date" type="date" class="field mt-1" @change="filterChanged" />
    </label>
    <label class="block">
      <span class="text-sm font-semibold">Payment</span>
      <select v-model="paid" class="field mt-1" @change="filterChanged">
        <option value="">All</option>
        <option value="paid">Paid</option>
        <option value="partly_paid">Partly paid</option>
        <option value="unpaid">Unpaid</option>
      </select>
    </label>
  </div>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
  <p v-else-if="page && page.total === 0" class="mt-6 text-muted">
    No sales{{ date ? ` on ${formatDay(date)}` : '' }}{{ paid ? ' with that payment status' : '' }}.
  </p>

  <ul v-else-if="page" class="card mt-6 divide-y divide-line">
    <li v-for="o in page.items" :key="o.id" class="relative grid grid-cols-[1fr_auto] gap-x-4 px-4 py-3.5 hover:bg-canvas/60">
      <div class="min-w-0">
        <RouterLink :to="`/sales/${o.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">{{ o.number }}</RouterLink>
        <p class="mt-0.5 truncate text-sm text-muted">
          {{ formatTime(o.created_at) }} · {{ o.customer_name || o.customer_phone || 'Walk-in' }}
        </p>
      </div>
      <div class="text-right">
        <p class="figure text-lg">{{ formatRupees(o.total) }}</p>
        <Badge v-if="o.status === 'cancelled'" tone="neutral">Cancelled</Badge>
        <Badge v-else-if="owedNow(o)" tone="golden">Owed {{ formatRupees(o.owed) }}</Badge>
      </div>
    </li>
  </ul>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex justify-end gap-2">
    <button type="button" class="btn-secondary" :disabled="pageNo <= 1" @click="(pageNo--, load())">Previous</button>
    <button type="button" class="btn-secondary" :disabled="pageNo >= lastPage" @click="(pageNo++, load())">Next</button>
  </nav>
</template>
