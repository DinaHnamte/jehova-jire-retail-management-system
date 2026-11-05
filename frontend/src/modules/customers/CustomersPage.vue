<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import { searchCustomers, type CustomerRow } from '@/modules/pos/api'
import { fmtPhone } from '@/modules/sales/format'

const q = ref('')
const items = ref<CustomerRow[] | null>(null)
const total = ref(0)
const error = ref<string | null>(null)
let ctl: AbortController | null = null
let timer: ReturnType<typeof setTimeout> | undefined

async function load() {
  ctl?.abort()
  ctl = new AbortController()
  error.value = null
  try {
    const r = await searchCustomers(q.value.trim(), ctl.signal)
    items.value = r.items
    total.value = r.total
  } catch (e) {
    if ((e as Error).name !== 'AbortError') error.value = friendlyMessage(e)
  }
}
onMounted(load)
watch(q, () => {
  clearTimeout(timer)
  timer = setTimeout(load, 250)
})
onBeforeUnmount(() => {
  clearTimeout(timer)
  ctl?.abort()
})
</script>

<template>
  <h1 class="text-3xl font-extrabold">Customers</h1>
  <label class="mt-4 block sm:max-w-md">
    <span class="sr-only">Search customers</span>
    <input v-model="q" type="search" class="field" placeholder="Search by name or phone" autocomplete="off" />
  </label>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
  <p v-else-if="items && items.length === 0" class="mt-6 text-muted">{{ q ? 'No customers match that.' : 'No customers yet. They are added when you sell to someone.' }}</p>

  <ul v-else-if="items" class="card mt-6 divide-y divide-line">
    <li v-for="c in items" :key="c.id" class="relative grid grid-cols-[1fr_auto] items-center gap-x-4 px-4 py-3.5 hover:bg-canvas/60">
      <div class="min-w-0">
        <RouterLink :to="`/customers/${c.id}`" class="block truncate font-bold after:absolute after:inset-0 hover:text-primary-ink">{{ c.name || fmtPhone(c.phone) }}</RouterLink>
        <p v-if="c.name" class="text-sm text-muted">{{ fmtPhone(c.phone) }}</p>
      </div>
      <Badge v-if="toPaise(c.owes) > 0n" tone="golden">Owes {{ formatRupees(c.owes) }}</Badge>
    </li>
  </ul>
  <p v-if="items && total > items.length" class="mt-3 text-sm text-muted">Showing {{ items.length }} of {{ total }}. Search to narrow down.</p>
</template>
