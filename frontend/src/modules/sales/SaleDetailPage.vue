<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees, fromPaise, toPaise } from '@/shared/money'
import { openWhatsApp } from '@/shared/whatsapp'
import Badge from '@/shared/ui/Badge.vue'
import PaymentDialog from '@/modules/customers/PaymentDialog.vue'
import CancelDialog from './CancelDialog.vue'
import ReturnDialog from './ReturnDialog.vue'
import { getCustomer, getOrder, type OrderDetail } from '@/modules/pos/api'
import { formatWhen } from '@/modules/stock/format'
import { fmtPhone, statusLabel } from './format'
import { forgetOrderAttempts } from './undo'
import { saleCancelNote, statusLabel as deliveryStatus } from '@/modules/deliveries/labels'

const route = useRoute()
const session = useSessionStore()
const order = ref<OrderDetail | null>(null)
const customerOwes = ref('0.00')
const error = ref<string | null>(null)
const dialog = ref<InstanceType<typeof PaymentDialog> | null>(null)
const cancelDialog = ref<InstanceType<typeof CancelDialog> | null>(null)
const returnDialog = ref<InstanceType<typeof ReturnDialog> | null>(null)

async function load() {
  error.value = null
  try {
    const o = await getOrder(String(route.params.id))
    customerOwes.value = o.customer ? (await getCustomer(o.customer.id)).owes : '0.00'
    // Nothing more can be cancelled or returned on a cancelled sale: drop any pending attempt.
    if (o.status === 'cancelled') forgetOrderAttempts(o.id)
    order.value = o
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

const owed = computed(() => (order.value ? toPaise(order.value.owed) > 0n : false))
const returned = computed(() => (order.value ? order.value.returns.reduce((s, r) => s + toPaise(r.amount), 0n) : 0n))
// Everything handed back in cash or UPI (return refunds, and a cancel's hand-back).
const refunded = computed(() => (order.value ? order.value.refunds.reduce((s, r) => s + toPaise(r.amount), 0n) : 0n))
const totalNow = computed(() => (order.value ? toPaise(order.value.total) - returned.value : 0n))
// The payment goes against the customer's whole balance (oldest sale first), not just this sale.
const canPay = computed(() => owed.value && !!order.value?.customer && order.value.status !== 'cancelled' && session.can('payments.record'))
const deliveryState = computed(() => order.value?.delivery?.status ?? null)
// A delivery that is out, failed or done can't be cancelled here (Ruling 4).
const cancelNote = computed(() => (deliveryState.value ? saleCancelNote(deliveryState.value) : null))
const canCancel = computed(
  () =>
    order.value?.status === 'confirmed' &&
    order.value.returns.length === 0 &&
    session.can('orders.void') &&
    !cancelNote.value,
)
const canReturn = computed(
  () =>
    order.value?.status === 'confirmed' &&
    (!order.value.delivery || order.value.delivery.status === 'delivered') &&
    order.value.lines.some((l) => l.quantity - l.returned_quantity > 0) &&
    session.can('returns.create'),
)
</script>

<template>
  <RouterLink to="/sales" class="text-muted hover:text-ink">← Sales</RouterLink>
  <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <section v-else-if="order" class="card mx-auto mt-3 max-w-md p-5 sm:p-6" aria-labelledby="sale-h">
    <div class="flex flex-wrap items-center gap-2">
      <h1 id="sale-h" class="text-xl font-bold">{{ order.number }}</h1>
      <Badge v-if="order.status === 'cancelled'" tone="neutral">Cancelled</Badge>
      <Badge v-else :tone="order.payment_status === 'paid' ? 'sage' : order.payment_status === 'returned' ? 'neutral' : 'golden'" dot>{{ statusLabel[order.payment_status] }}</Badge>
    </div>
    <p class="mt-1 text-sm text-muted">{{ formatWhen(order.created_at) }}</p>
    <p v-if="order.status === 'cancelled'" role="status" class="mt-3 rounded-control bg-canvas px-4 py-3 font-semibold text-muted">
      This sale was cancelled. The items are back on the shelf<template v-if="order.refunds.length">
        and {{ order.refunds.map((r) => `${formatRupees(r.amount)} ${r.method === 'upi' ? 'UPI' : r.method} was handed back`).join(', ') }}</template>.
    </p>
    <p v-if="order.customer" class="mt-1 text-sm">
      <RouterLink :to="`/customers/${order.customer.id}`" class="link">{{ order.customer.name || fmtPhone(order.customer.phone) }}</RouterLink>
      <span v-if="order.customer.name" class="text-muted"> · {{ fmtPhone(order.customer.phone) }}</span>
    </p>

    <p v-if="order.delivery" class="mt-3 rounded-control bg-canvas px-4 py-3 text-sm">
      <span class="font-semibold">Delivery · {{ deliveryStatus(order.delivery.status) }}</span>
      <template v-if="order.delivery.driver_name"> · {{ order.delivery.driver_name }}</template>
      · <RouterLink :to="`/deliveries/${order.delivery.id}`" class="link">Open delivery</RouterLink>
    </p>

    <ul class="mt-4 divide-y divide-line border-y border-line">
      <li v-for="l in order.lines" :key="l.product_id" class="grid grid-cols-[1fr_auto] gap-x-3 py-2.5">
        <div class="min-w-0">
          <p class="font-semibold">{{ l.product_name }}</p>
          <p class="text-sm text-muted">{{ l.quantity }} × {{ formatRupees(l.unit_price) }}<template v-if="l.returned_quantity > 0"> · {{ l.returned_quantity }} returned</template></p>
        </div>
        <p class="figure self-center">{{ formatRupees(l.net_amount) }}</p>
      </li>
    </ul>

    <dl class="mt-3 grid grid-cols-[1fr_auto] gap-y-1.5">
      <template v-if="toPaise(order.order_discount) > 0n || toPaise(order.delivery_fee) > 0n">
        <dt class="text-muted">Subtotal</dt><dd class="figure">{{ formatRupees(order.subtotal) }}</dd>
      </template>
      <template v-if="toPaise(order.order_discount) > 0n">
        <dt class="text-muted">Discount</dt><dd class="figure">−{{ formatRupees(order.order_discount) }}</dd>
      </template>
      <template v-if="toPaise(order.delivery_fee) > 0n">
        <dt class="text-muted">Delivery</dt><dd class="figure">{{ formatRupees(order.delivery_fee) }}</dd>
      </template>
      <dt class="font-bold">Total</dt><dd class="figure text-xl">{{ formatRupees(order.total) }}</dd>
      <template v-if="returned > 0n">
        <dt class="text-muted">Returned</dt><dd class="figure">−{{ formatRupees(fromPaise(returned)) }}</dd>
        <dt class="font-bold">Now</dt><dd class="figure font-bold">{{ formatRupees(fromPaise(totalNow)) }}</dd>
      </template>
      <dt class="text-muted">Paid</dt><dd class="figure">{{ formatRupees(order.paid) }}</dd>
      <dt class="text-muted">Owed</dt><dd class="figure" :class="owed ? 'font-bold text-golden-ink' : ''">{{ formatRupees(order.owed) }}</dd>
      <template v-if="refunded > 0n">
        <dt class="text-muted">Refunded</dt><dd class="figure">{{ formatRupees(fromPaise(refunded)) }}</dd>
      </template>
    </dl>

    <p v-if="order.note" class="mt-3 text-sm text-muted">Note: {{ order.note }}</p>

    <div v-if="order.payments.length" class="mt-4">
      <h2 class="font-semibold">Payments</h2>
      <ul class="mt-1 text-sm text-muted">
        <li v-for="p in order.payments" :key="p.id" class="flex justify-between">
          <span>{{ formatWhen(p.occurred_at) }} · {{ p.method.toUpperCase() }}</span>
          <span class="figure">{{ formatRupees(p.amount) }}</span>
        </li>
      </ul>
    </div>

    <div v-if="order.refunds.length" class="mt-4">
      <h2 class="font-semibold">Handed back</h2>
      <ul class="mt-1 text-sm text-muted">
        <li v-for="(r, i) in order.refunds" :key="i" class="flex justify-between">
          <span>{{ formatWhen(r.occurred_at) }} · {{ r.method.toUpperCase() }}</span>
          <span class="figure">{{ formatRupees(r.amount) }}</span>
        </li>
      </ul>
    </div>

    <div v-if="order.returns.length" class="mt-4">
      <h2 class="font-semibold">Returns</h2>
      <ul class="mt-1 divide-y divide-line text-sm">
        <li v-for="r in order.returns" :key="r.id" class="py-2">
          <p class="flex justify-between text-muted">
            <span>{{ formatWhen(r.created_at) }}</span>
            <span class="figure">{{ formatRupees(r.amount) }}</span>
          </p>
          <p v-for="l in r.lines" :key="l.order_line_id" class="text-muted">
            {{ l.quantity }} × {{ l.product_name }} ({{ l.disposition === 'damaged' ? 'damaged' : 'sellable' }})
          </p>
          <p v-if="toPaise(r.owed_reduced) > 0n" class="text-muted">Took {{ formatRupees(r.owed_reduced) }} off what they owed</p>
          <p v-if="toPaise(r.refunded) > 0n" class="text-muted">Refunded {{ formatRupees(r.refunded) }}<template v-if="r.refund_method"> by {{ r.refund_method.toUpperCase() }}</template></p>
          <p v-if="r.note" class="text-muted">Note: {{ r.note }}</p>
        </li>
      </ul>
    </div>

    <div class="mt-6 grid gap-3">
      <button v-if="order.whatsapp_url" type="button" class="btn-primary" @click="openWhatsApp(order.whatsapp_url)">Send on WhatsApp</button>
      <button v-if="canPay" type="button" :class="order.whatsapp_url ? 'btn-secondary' : 'btn-primary'" @click="dialog?.open()">Record payment</button>
      <button v-if="canReturn" type="button" class="btn-secondary" @click="returnDialog?.open()">Return items</button>
      <button v-if="canCancel" type="button" class="btn-secondary" @click="cancelDialog?.open()">Cancel sale</button>
      <p v-if="cancelNote && order.status === 'confirmed'" class="text-sm text-muted">{{ cancelNote }}</p>
    </div>
  </section>

  <CancelDialog
    v-if="order"
    ref="cancelDialog"
    :order-id="order.id"
    :number="order.number"
    :cash="order.paid_by_method.cash"
    :upi="order.paid_by_method.upi"
    @cancelled="load"
    @stale="load"
  />
  <ReturnDialog
    v-if="order"
    ref="returnDialog"
    :order-id="order.id"
    :number="order.number"
    :lines="order.lines"
    @returned="load"
    @stale="load"
  />
  <PaymentDialog v-if="order?.customer" ref="dialog" :customer-id="order.customer.id" :owes="customerOwes" @saved="load" />
</template>
