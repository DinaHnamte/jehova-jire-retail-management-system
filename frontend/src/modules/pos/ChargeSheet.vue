<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, fromPaise, parseMoneyInput, toPaise } from '@/shared/money'
import { fieldToLineIndex } from '@/modules/stock/totals'
import { checkout, type CheckoutPayment, type OrderDetail } from './api'
import { useCartStore } from './cart.store'
import CustomerPicker from './CustomerPicker.vue'

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

function open() {
  if (dialog.value?.open) return
  cash.value = store.totals.total
  upi.value = ''
  error.value = null
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
const needsCustomer = computed(() => owing.value > 0n && !hasCustomer.value)
const canCharge = computed(() => store.valid && inputsOk.value && !overpaid.value && !needsCustomer.value && !busy.value)

function payLater() {
  cash.value = '0'
  upi.value = '0'
}

async function charge() {
  if (!canCharge.value) return
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
  const message = friendlyMessage(e)
  if (e instanceof ApiError) {
    const details = (e.details ?? {}) as { field?: string; product_id?: string }
    let line = details.product_id ? store.lines.find((l) => l.productId === details.product_id) : undefined
    const idx = fieldToLineIndex(details.field)
    if (!line && idx) line = store.lines[idx.index]
    // INSUFFICIENT_STOCK names the product in its message ("Only 3 of B 6143-8 in stock...").
    if (!line && e.code === 'INSUFFICIENT_STOCK')
      line = store.lines.find((l) => (l.code && message.includes(l.code)) || message.includes(l.name))
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
      <p class="mt-1 text-sm text-muted">{{ store.count }} item{{ store.count === 1 ? '' : 's' }}</p>

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
          <span v-if="owing > 0n" class="font-bold text-golden-ink">Left owing {{ formatRupees(fromPaise(owing)) }}</span>
          <span v-else class="font-bold text-sage-ink">Paid in full</span>
        </p>
      </div>

      <div v-if="owing > 0n" class="mt-4">
        <CustomerPicker />
        <p v-if="needsCustomer" class="mt-2 text-sm font-medium text-golden-ink">Pick a customer to leave money owing.</p>
      </div>
      <p v-else-if="hasCustomer" class="mt-4 text-sm text-muted">
        For {{ store.customer?.name || store.customer?.phone || store.newCustomer?.name || store.newCustomer?.phone }}
      </p>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="dialog?.close()">Back</button>
        <button type="submit" class="btn-primary" :disabled="!canCharge">{{ busy ? 'Charging…' : 'Charge' }}</button>
      </div>
    </form>
  </dialog>
</template>
