<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, fromPaise, parseMoneyInput, toPaise } from '@/shared/money'
import { fieldToLineIndex } from '@/modules/stock/totals'
import { checkout, lastAddress, type CheckoutPayment, type OrderDetail } from './api'
import { useCartStore } from './cart.store'
import CustomerPicker from './CustomerPicker.vue'
import DeliveryFields from './DeliveryFields.vue'
import { draftErrors, draftFromLastAddress } from './delivery'

const emit = defineEmits<{
  done: [order: OrderDetail]
  /** The server named a cart line; the page marks it and the sheet closes. */
  lineError: [productId: string, message: string]
}>()

const store = useCartStore()
const dialog = ref<HTMLDialogElement | null>(null)
const cash = ref('')
const upi = ref('')
const error = ref<string | null>(null)
const busy = ref(false)
/** A charge was sent with no known result; it is shown as it was so a retry reuses its key. */
const pending = ref(false)
const isDelivery = computed(() => store.fulfilment === 'delivery')
/** Server complaints about delivery inputs, shown on the matching input until it is edited. */
const serverErrors = ref<Record<string, string>>({})
const customerError = ref<string | null>(null)
const showErrors = ref(false)
const localErrors = computed(() => (isDelivery.value ? draftErrors(store.delivery) : {}))
const fieldErrors = computed(() => ({ ...(showErrors.value ? localErrors.value : {}), ...serverErrors.value }))
watch(() => store.delivery, () => (serverErrors.value = {}))
watch(() => [store.customer, store.newCustomer], () => (customerError.value = null))

function setMode(mode: 'counter' | 'delivery') {
  if (store.fulfilment === mode) return
  store.setFulfilment(mode)
  serverErrors.value = {}
  showErrors.value = false
  // Delivery: the driver collects by default. Counter: paid in full by default.
  cash.value = mode === 'delivery' ? '0' : store.totals.total
  upi.value = ''
  if (mode === 'delivery') prefill()
}

/** Fills only the empty delivery fields: from the last delivery of a saved customer, else the customer itself. */
async function prefill() {
  const c = store.customer
  const who = c ?? store.newCustomer
  if (!who) return
  const base = { name: who.name || null, phone: who.phone }
  let addr = null
  if (c) {
    try {
      addr = await lastAddress(c.id)
    } catch {
      // No earlier delivery (404) or no connection: the customer's own details are used.
    }
  }
  if (store.customer !== c) return
  store.setDelivery(draftFromLastAddress(addr, base, store.delivery))
}
watch(() => [store.customer?.id, store.newCustomer?.phone], () => {
  if (isDelivery.value && dialog.value?.open) prefill()
})

function open() {
  if (dialog.value?.open) return
  const sent = store.pendingPayments
  pending.value = sent !== null
  if (sent) {
    cash.value = sent.find((p) => p.method === 'cash')?.amount ?? '0'
    upi.value = sent.find((p) => p.method === 'upi')?.amount ?? ''
  } else {
    cash.value = isDelivery.value ? '0' : store.totals.total
    upi.value = ''
  }
  error.value = null
  serverErrors.value = {}
  customerError.value = null
  showErrors.value = false
  dialog.value?.showModal()
  nextTick(() => dialog.value?.querySelector<HTMLInputElement>('input[name=cash]')?.select())
}
defineExpose({ open, isOpen: () => !!dialog.value?.open })

/** A blank field is zero; text that isn't an amount is null. */
const amount = (s: string) => (s.trim() === '' ? '0.00' : parseMoneyInput(s))
const cashAmt = computed(() => amount(cash.value))
const upiAmt = computed(() => amount(upi.value))
const inputsOk = computed(() => cashAmt.value !== null && upiAmt.value !== null)
const paidPaise = computed(() => (inputsOk.value ? toPaise(cashAmt.value!) + toPaise(upiAmt.value!) : 0n))
const totalPaise = computed(() => toPaise(store.totals.total))
const overpaid = computed(() => paidPaise.value > totalPaise.value)
const owing = computed(() => (overpaid.value ? 0n : totalPaise.value - paidPaise.value))
const hasCustomer = computed(() => !!(store.customer || store.newCustomer))
const needsCustomer = computed(() => (owing.value > 0n || isDelivery.value) && !hasCustomer.value)
const deliveryOk = computed(() => Object.keys(localErrors.value).length === 0)
const canCharge = computed(() => store.valid && inputsOk.value && deliveryOk.value && !overpaid.value && !needsCustomer.value && !busy.value)

function payLater() {
  cash.value = '0'
  upi.value = '0'
}

async function charge() {
  if (!canCharge.value) {
    showErrors.value = true
    return
  }
  error.value = null
  busy.value = true
  const payments: CheckoutPayment[] = []
  if (toPaise(cashAmt.value!) > 0n) payments.push({ method: 'cash', amount: cashAmt.value! })
  if (toPaise(upiAmt.value!) > 0n) payments.push({ method: 'upi', amount: upiAmt.value! })
  try {
    const order = await checkout(store.toBody(payments), store.chargeKey(payments))
    dialog.value?.close()
    emit('done', order)
  } catch (e) {
    handleError(e)
  } finally {
    busy.value = false
  }
}

function handleError(e: unknown) {
  if (e instanceof ApiError && e.code === 'NETWORK_ERROR') {
    // The same body goes with the same key, so pressing Charge again is safe.
    error.value = "No connection. Press Charge again — it won't charge twice."
    return
  }
  const message = friendlyMessage(e)
  if (e instanceof ApiError) {
    const details = (e.details ?? {}) as { field?: string; product_id?: string }
    if (details.field?.startsWith('delivery.')) {
      serverErrors.value = { [details.field]: message }
      nextTick(() => dialog.value?.querySelector<HTMLInputElement>(`[name="${details.field}"]`)?.focus())
      return
    }
    if (details.field === 'customer_id') {
      customerError.value = message
      return
    }
    let line = details.product_id ? store.lines.find((l) => l.productId === details.product_id) : undefined
    const idx = fieldToLineIndex(details.field)
    if (!line && idx) line = store.lines[idx.index]
    if (line) {
      dialog.value?.close()
      emit('lineError', line.productId, message)
      return
    }
  }
  error.value = message
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="charge-title"
    class="m-auto w-[min(100%-1rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
  >
    <form class="p-5" @submit.prevent="charge">
      <h2 id="charge-title" class="text-xl font-bold">Charge</h2>
      <p class="figure mt-2 text-4xl">{{ formatRupees(store.totals.total) }}</p>
      <p v-if="isDelivery" class="mt-1 text-sm text-muted">
        Goods {{ formatRupees(store.goods.total) }} + Delivery {{ formatRupees(fromPaise(totalPaise - toPaise(store.goods.total))) }} = {{ formatRupees(store.totals.total) }}
      </p>
      <p class="mt-1 text-sm text-muted">{{ store.count }} item{{ store.count === 1 ? '' : 's' }}</p>

      <div class="mt-4 grid grid-cols-2 gap-2" role="radiogroup" aria-label="Sale type">
        <button type="button" role="radio" :aria-checked="!isDelivery" :disabled="pending" :class="!isDelivery ? 'btn-primary' : 'btn-secondary'" @click="setMode('counter')">Counter</button>
        <button type="button" role="radio" :aria-checked="isDelivery" :disabled="pending" :class="isDelivery ? 'btn-primary' : 'btn-secondary'" @click="setMode('delivery')">Delivery</button>
      </div>

      <p v-if="pending && !error" role="status" class="mt-4 rounded-control bg-golden-tint px-4 py-3 font-medium text-golden-ink">
        This sale may already have gone through. Check Sales before changing anything.
      </p>
      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

      <div class="mt-4 grid grid-cols-2 gap-3">
        <label class="block">
          <span class="font-semibold">Cash ₹</span>
          <input v-model="cash" name="cash" class="field mt-1" inputmode="decimal" autocomplete="off" :aria-invalid="cashAmt === null" />
        </label>
        <label class="block">
          <span class="font-semibold">UPI ₹</span>
          <input v-model="upi" name="upi" class="field mt-1" inputmode="decimal" autocomplete="off" placeholder="0" :aria-invalid="upiAmt === null" />
        </label>
      </div>
      <p v-if="!inputsOk" class="mt-1 text-sm font-medium text-danger">Enter amounts like 500 or 499.50.</p>
      <p v-else-if="overpaid" class="mt-1 text-sm font-medium text-danger">
        That is {{ formatRupees(fromPaise(paidPaise - totalPaise)) }} more than the total. Lower the amounts.
      </p>

      <div class="mt-3 flex items-center justify-between gap-3">
        <button type="button" class="btn-secondary" @click="payLater">Pay later</button>
        <p v-if="inputsOk && !overpaid" class="text-right">
          <span v-if="owing > 0n && isDelivery" class="font-bold text-golden-ink">Driver collects {{ formatRupees(fromPaise(owing)) }}</span>
          <span v-else-if="owing > 0n" class="font-bold text-golden-ink">Left owing {{ formatRupees(fromPaise(owing)) }}</span>
          <span v-else class="font-bold text-sage-ink">Paid in full</span>
        </p>
      </div>

      <div v-if="owing > 0n || isDelivery" class="mt-4">
        <CustomerPicker />
        <p v-if="customerError" role="alert" class="mt-2 text-sm font-medium text-danger">{{ customerError }}</p>
        <p v-if="needsCustomer && isDelivery" class="mt-2 text-sm font-medium text-golden-ink">Pick or add the customer for this delivery.</p>
        <p v-else-if="needsCustomer" class="mt-2 text-sm font-medium text-golden-ink">Pick a customer to leave money owing.</p>
      </div>
      <p v-else-if="hasCustomer" class="mt-4 text-sm text-muted">
        For {{ store.customer?.name || store.customer?.phone || store.newCustomer?.name || store.newCustomer?.phone }}
      </p>

      <DeliveryFields v-if="isDelivery" :model-value="store.delivery" :errors="fieldErrors" @update:model-value="store.setDelivery" />

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="dialog?.close()">Back</button>
        <button type="submit" class="btn-primary" :disabled="!canCharge">{{ busy ? 'Charging…' : 'Charge' }}</button>
      </div>
    </form>
  </dialog>
</template>
