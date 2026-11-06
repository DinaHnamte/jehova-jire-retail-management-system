<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import { openWhatsApp } from '@/shared/whatsapp'
import Badge from '@/shared/ui/Badge.vue'
import CancelDialog from '@/modules/sales/CancelDialog.vue'
import { forgetOrderAttempts } from '@/modules/sales/undo'
import { getOrder } from '@/modules/pos/api'
import { formatWhen } from '@/modules/stock/format'
import { fmtPhone } from '@/modules/sales/format'
import { getDelivery, type DeliveryDetail } from './api'
import { actionsFor, reasonLabel, statusLabel, type DeliveryAction } from './labels'
import AssignDialog from './AssignDialog.vue'
import CheckInDialog from './CheckInDialog.vue'
import EditDeliveryDialog from './EditDeliveryDialog.vue'
import StatusDialog from './StatusDialog.vue'

const route = useRoute()
const session = useSessionStore()
const d = ref<DeliveryDetail | null>(null)
const paid = ref({ cash: '0.00', upi: '0.00' })
const error = ref<string | null>(null)
const notice = ref<string | null>(null)

const assignDialog = ref<InstanceType<typeof AssignDialog> | null>(null)
const editDialog = ref<InstanceType<typeof EditDeliveryDialog> | null>(null)
const checkInDialog = ref<InstanceType<typeof CheckInDialog> | null>(null)
const cancelDialog = ref<InstanceType<typeof CancelDialog> | null>(null)
const pickedUp = ref<InstanceType<typeof StatusDialog> | null>(null)
const delivered = ref<InstanceType<typeof StatusDialog> | null>(null)
const failed = ref<InstanceType<typeof StatusDialog> | null>(null)

async function load() {
  try {
    const x = await getDelivery(String(route.params.id))
    const o = await getOrder(x.order_id)
    paid.value = o.paid_by_method
    if (x.status === 'cancelled' || x.status === 'returned') forgetOrderAttempts(x.order_id)
    d.value = x
    error.value = null
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

/** An action succeeded. */
function done() {
  notice.value = null
  void load()
}
/** The delivery had moved on: show why (if it isn't what we asked for) and reload. */
function stale(message: string | null) {
  notice.value = message
  void load()
}

const mode = computed(() => (d.value?.status === 'failed' ? 'retry' : d.value?.driver ? 'reassign' : 'assign'))
const actions = computed<DeliveryAction[]>(() => {
  if (!d.value) return []
  const all = actionsFor(d.value.status, !!d.value.driver)
  const need: Record<DeliveryAction, string> = {
    assign: 'deliveries.assign',
    reassign: 'deliveries.assign',
    retry: 'deliveries.assign',
    picked_up: 'deliveries.assign',
    delivered: 'deliveries.assign',
    failed: 'deliveries.assign',
    edit: 'deliveries.assign',
    cancel: 'deliveries.cancel',
    check_in: 'deliveries.check_in',
  }
  return all.filter((a) => session.can(need[a]))
})
const has = (a: DeliveryAction) => actions.value.includes(a)

const eventText = (e: DeliveryDetail['events'][number]) => {
  if (e.kind === 'edit') return `Edited by ${e.actor_name}${e.changes ? `: ${Object.keys(e.changes).join(', ').replace(/_/g, ' ')}` : ''}`
  return `${statusLabel(e.to)} · ${e.actor_name}`
}
</script>

<template>
  <RouterLink to="/deliveries" class="text-muted hover:text-ink">← Deliveries</RouterLink>
  <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <section v-else-if="d" class="card mx-auto mt-3 max-w-md p-5 sm:p-6" aria-labelledby="del-h">
    <div class="flex flex-wrap items-center gap-2">
      <h1 id="del-h" class="text-xl font-bold">Delivery</h1>
      <Badge :tone="d.status === 'delivered' ? 'sage' : d.status === 'failed' ? 'danger' : d.status === 'cancelled' || d.status === 'returned' ? 'neutral' : 'golden'" dot>{{ statusLabel(d.status) }}</Badge>
    </div>
    <RouterLink :to="`/sales/${d.order_id}`" class="link mt-1 inline-block text-sm">Sale {{ d.order.number }}</RouterLink>

    <p v-if="notice" role="alert" class="mt-3 rounded-control bg-golden-tint px-4 py-3 font-medium text-golden-ink">{{ notice }}</p>

    <div class="mt-4">
      <p class="font-semibold">{{ d.recipient_name }}</p>
      <p><a :href="`tel:${d.recipient_phone}`" class="link">{{ fmtPhone(d.recipient_phone) }}</a></p>
      <p class="mt-1">{{ d.address_line }}, {{ d.locality }}</p>
      <p v-if="d.landmark" class="text-sm text-muted">Near {{ d.landmark }}</p>
      <p v-if="d.instructions" class="text-sm text-muted">{{ d.instructions }}</p>
      <a :href="d.maps_url" target="_blank" rel="noopener" class="link mt-1 inline-block">Open in Maps</a>
    </div>

    <p v-if="d.driver" class="mt-3 text-sm">
      Driver: <span class="font-semibold">{{ d.driver.name }}</span>
      <Badge v-if="!d.driver.active" tone="danger">inactive</Badge>
      · <a :href="`tel:${d.driver.phone}`" class="link">{{ fmtPhone(d.driver.phone) }}</a>
    </p>
    <p v-if="d.received_by" class="mt-1 text-sm text-muted">Received by {{ d.received_by }}</p>

    <ul class="mt-4 divide-y divide-line border-y border-line">
      <li v-for="i in d.items" :key="i.order_line_id" class="py-2">{{ i.quantity }} × {{ i.name }}</li>
    </ul>

    <dl class="mt-3 grid grid-cols-[1fr_auto] gap-y-1.5">
      <dt class="text-muted">Delivery fee</dt><dd class="figure">{{ formatRupees(d.fee) }}</dd>
      <dt class="font-bold">Total</dt><dd class="figure text-xl">{{ formatRupees(d.order.total) }}</dd>
      <dt class="text-muted">Paid</dt><dd class="figure">{{ formatRupees(d.order.paid) }}</dd>
      <dt class="text-muted">Still owed</dt><dd class="figure">{{ formatRupees(d.order.owed) }}</dd>
      <dt class="text-muted">Collected by</dt><dd>{{ d.collector === 'driver' ? 'Driver' : 'Shop' }}</dd>
      <template v-if="d.collector === 'driver'">
        <dt class="text-muted">Driver brings back</dt><dd class="figure">{{ formatRupees(d.amount_to_collect) }}</dd>
      </template>
      <dt class="text-muted">Driver's pay</dt>
      <dd class="figure">{{ d.driver_pay ? formatRupees(d.driver_pay) : '—' }}<span v-if="d.driver_pay" class="text-sm text-muted"> · {{ d.pay_method === 'paid_by_shop' ? 'paid by shop' : 'kept from cash' }}</span></dd>
    </dl>

    <div class="mt-5 grid gap-3">
      <button type="button" class="btn-secondary" :disabled="!d.driver_whatsapp_url" @click="d.driver_whatsapp_url && openWhatsApp(d.driver_whatsapp_url)">Send to driver on WhatsApp</button>
      <button type="button" class="btn-secondary" :disabled="!d.customer_whatsapp_url" @click="d.customer_whatsapp_url && openWhatsApp(d.customer_whatsapp_url)">Tell customer it's on the way</button>
      <button v-if="has('assign')" type="button" class="btn-primary" @click="assignDialog?.open()">Assign driver</button>
      <button v-if="has('reassign')" type="button" class="btn-secondary" @click="assignDialog?.open()">Change driver</button>
      <button v-if="has('retry')" type="button" class="btn-primary" @click="assignDialog?.open()">Send again</button>
      <button v-if="has('picked_up')" type="button" class="btn-primary" @click="pickedUp?.open()">Mark picked up</button>
      <button v-if="has('delivered')" type="button" class="btn-primary" @click="delivered?.open()">Mark delivered</button>
      <button v-if="has('failed')" type="button" class="btn-secondary" @click="failed?.open()">Couldn't deliver</button>
      <button v-if="has('check_in')" type="button" class="btn-primary" @click="checkInDialog?.open()">Check goods back in</button>
      <button v-if="has('edit')" type="button" class="btn-secondary" @click="editDialog?.open()">Edit</button>
      <button v-if="has('cancel')" type="button" class="btn-secondary" @click="cancelDialog?.open()">Cancel delivery</button>
    </div>

    <div class="mt-6">
      <h2 class="font-semibold">History</h2>
      <ul class="mt-1 text-sm text-muted">
        <li v-for="(e, i) in d.events" :key="i" class="flex justify-between gap-3 py-0.5">
          <span>{{ eventText(e) }}<template v-if="e.note"> — {{ e.note }}</template></span>
          <span class="shrink-0">{{ formatWhen(e.at) }}</span>
        </li>
      </ul>
    </div>

    <div v-if="d.attempts.length" class="mt-4">
      <h2 class="font-semibold">Attempts</h2>
      <ul class="mt-1 text-sm text-muted">
        <li v-for="(a, i) in d.attempts" :key="i" class="py-0.5">
          {{ a.driver_name }} · {{ reasonLabel(a.reason) }}<template v-if="a.note"> — {{ a.note }}</template>
          <template v-if="a.trip_pay"> · trip pay {{ formatRupees(a.trip_pay) }}</template> · {{ formatWhen(a.at) }}
        </li>
      </ul>
    </div>
  </section>

  <template v-if="d">
    <AssignDialog ref="assignDialog" :delivery="d" :mode="mode" @done="done" @stale="stale" />
    <EditDeliveryDialog ref="editDialog" :delivery="d" @done="done" @stale="stale" />
    <StatusDialog ref="pickedUp" :delivery-id="d.id" to="picked_up" @done="done" @stale="stale" />
    <StatusDialog ref="delivered" :delivery-id="d.id" to="delivered" @done="done" @stale="stale" />
    <StatusDialog ref="failed" :delivery-id="d.id" to="failed" @done="done" @stale="stale" />
    <CheckInDialog ref="checkInDialog" :delivery-id="d.id" :items="d.items" @done="done" @stale="stale" />
    <CancelDialog
      ref="cancelDialog"
      :order-id="d.order_id"
      :number="d.order.number"
      :cash="paid.cash"
      :upi="paid.upi"
      @cancelled="done"
      @stale="stale(null)"
    />
  </template>
</template>
