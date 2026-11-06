<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import { listDeliveries, type DeliveryList, type DeliveryTab } from './api'
import { ageLabel, reasonLabel, statusLabel } from './labels'

const tabs: { id: DeliveryTab; label: string }[] = [
  { id: 'to_send', label: 'To send' },
  { id: 'out', label: 'Out' },
  { id: 'failed', label: 'Failed' },
  { id: 'done', label: 'Done' },
]

const route = useRoute()
const router = useRouter()
const tab = computed<DeliveryTab>(() => {
  const t = String(route.query.tab ?? '')
  return tabs.some((x) => x.id === t) ? (t as DeliveryTab) : 'to_send'
})
const data = ref<DeliveryList | null>(null)
const error = ref<string | null>(null)
let seq = 0

async function load() {
  const mine = ++seq
  error.value = null
  try {
    const r = await listDeliveries(tab.value)
    if (mine === seq) data.value = r
  } catch (e) {
    if (mine === seq) error.value = friendlyMessage(e)
  }
}
onMounted(load)
watch(tab, () => {
  data.value = null
  void load()
})

function pick(t: DeliveryTab) {
  void router.replace({ query: { ...route.query, tab: t } })
}
const count = (t: DeliveryTab) => (t === 'done' ? null : data.value?.counts[t])
const collects = (a: string) => toPaise(a) > 0n
</script>

<template>
  <h1 class="text-3xl font-extrabold">Deliveries</h1>

  <div class="mt-4 grid grid-cols-4 gap-1 rounded-control bg-canvas p-1" role="tablist" aria-label="Deliveries">
    <button
      v-for="t in tabs"
      :key="t.id"
      type="button"
      role="tab"
      :aria-selected="tab === t.id"
      class="min-h-11 rounded-control px-1 text-sm font-semibold"
      :class="tab === t.id ? 'bg-surface text-ink shadow-card' : 'text-muted'"
      @click="pick(t.id)"
    >
      {{ t.label }}<span v-if="count(t.id)" class="ml-1 rounded-full bg-primary-tint px-1.5 text-primary-ink">{{ count(t.id) }}</span>
    </button>
  </div>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
  <p v-else-if="data && data.items.length === 0" class="mt-6 text-muted">
    {{ tab === 'to_send' ? 'Nothing waiting to be sent.' : tab === 'out' ? 'Nothing is out for delivery.' : tab === 'failed' ? 'No failed deliveries.' : 'Nothing finished in the last 7 days.' }}
  </p>

  <ul v-else-if="data" class="card mt-4 divide-y divide-line">
    <li v-for="d in data.items" :key="d.id" class="relative grid grid-cols-[1fr_auto] gap-x-4 px-4 py-3.5 hover:bg-canvas/60">
      <div class="min-w-0">
        <RouterLink :to="`/deliveries/${d.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">{{ d.order_number }}</RouterLink>
        <p class="mt-0.5 truncate text-sm">{{ d.recipient_name }} · {{ d.locality }}</p>
        <p class="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted">
          <span v-if="d.driver">{{ d.driver.name }}</span>
          <span v-else>No driver yet</span>
          <Badge v-if="d.driver && !d.driver.active" tone="danger">inactive</Badge>
          <span v-if="tab === 'done' || tab === 'failed'">{{ statusLabel(d.status) }}<template v-if="d.fail_reason"> · {{ reasonLabel(d.fail_reason) }}</template></span>
        </p>
      </div>
      <div class="text-right">
        <p class="figure text-lg">{{ collects(d.amount_to_collect) ? formatRupees(d.amount_to_collect) : 'Paid' }}</p>
        <p class="text-sm text-muted">{{ ageLabel(d.since) }}</p>
      </div>
    </li>
  </ul>
</template>
