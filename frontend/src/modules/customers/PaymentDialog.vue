<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, parseMoneyInput } from '@/shared/money'
import { recordPayment, type CustomerPaymentBody, type PayMethod } from '@/modules/pos/api'
import { describePayment, makePaymentSession } from './payment-key'

const props = defineProps<{ customerId: string; owes: string }>()
const emit = defineEmits<{ saved: [] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const method = ref<PayMethod>('cash')
const amount = ref('')
const note = ref('')
const errors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const result = ref<string | null>(null)
const busy = ref(false)
// An earlier payment that got no answer: the user must choose to retry it or start a new one.
const pending = ref<CustomerPaymentBody | null>(null)
// One session per customer. A payment that got no answer stays pending (also across a reload)
// until the server answers it or the user starts a new payment.
let session = makePaymentSession(props.customerId)
watch(
  () => props.customerId,
  () => (session = makePaymentSession(props.customerId)),
)

function fill(p: CustomerPaymentBody | null) {
  method.value = p?.method ?? 'cash'
  amount.value = (p ? p.amount : props.owes).replace(/\.00$/, '')
  note.value = p?.note ?? ''
}

async function focusAmount() {
  await nextTick()
  dialog.value?.querySelector<HTMLInputElement>('input[name=amount]')?.select()
}

async function open() {
  pending.value = session.open()
  fill(null)
  errors.value = {}
  formError.value = null
  result.value = null
  dialog.value?.showModal()
  if (!pending.value) await focusAmount()
}

/** Send the earlier payment again as it was: same body, same key, so it is recorded once. */
async function retryPending() {
  fill(session.retry())
  pending.value = null
  await focusAmount()
}

/** The earlier payment is a separate matter: this one gets a new key. */
async function startNew() {
  session.startNew()
  pending.value = null
  fill(null)
  await focusAmount()
}
defineExpose({ open })

async function save() {
  if (session.needsChoice()) return
  errors.value = {}
  formError.value = null
  const a = parseMoneyInput(amount.value)
  if (a === null || a === '0.00') {
    errors.value.amount = 'Enter an amount in rupees, like 200 or 200.50.'
    return
  }
  const body = { method: method.value, amount: a, ...(note.value.trim() ? { note: note.value.trim() } : {}) }
  busy.value = true
  try {
    const r = await recordPayment(props.customerId, body, session.key(body))
    session.done()
    result.value = `Applied to ${r.applied.map((x) => `${x.number} ${formatRupees(x.amount)}`).join(', ')}`
    emit('saved')
  } catch (e) {
    // A 4xx is the server's final answer for this body: a fresh attempt gets a fresh key.
    if (e instanceof ApiError && e.status >= 400 && e.status < 500) session.done()
    const field = e instanceof ApiError ? (e.details as { field?: string })?.field : undefined
    if (e instanceof ApiError && e.code === 'OVERPAYMENT') errors.value.amount = e.message
    else if (field === 'amount' || field === 'method') errors.value[field] = friendlyMessage(e)
    else if (e instanceof ApiError && e.code === 'NETWORK_ERROR')
      formError.value = "No connection. Press Record payment again — it won't record twice."
    else formError.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="pay-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
  >
    <div v-if="result" class="p-5 sm:p-6">
      <h2 id="pay-title" class="text-xl font-bold">Payment recorded</h2>
      <p role="status" class="mt-3 rounded-control bg-sage-tint px-4 py-3 font-medium text-sage-ink">{{ result }}</p>
      <div class="mt-6 flex justify-end">
        <button type="button" class="btn-primary" @click="dialog?.close()">Done</button>
      </div>
    </div>
    <form v-else class="p-5 sm:p-6" @submit.prevent="save">
      <h2 id="pay-title" class="text-xl font-bold">Record payment</h2>
      <p class="mt-1 text-muted">Owes now: <span class="figure font-bold text-ink">{{ formatRupees(owes) }}</span></p>

      <div v-if="pending" role="alert" class="mt-4 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        <p class="font-semibold">Your last payment of {{ describePayment(pending, formatRupees) }} may not have gone through.</p>
        <div class="mt-3 grid gap-2 sm:grid-cols-2">
          <button type="button" class="btn-primary" @click="retryPending">Retry it</button>
          <button type="button" class="btn-secondary" @click="startNew">Start a new payment</button>
        </div>
      </div>

      <fieldset class="mt-4" :disabled="!!pending">
        <legend class="font-semibold">Paid by</legend>
        <div class="mt-2 grid grid-cols-2 gap-2">
          <label
            v-for="m in (['cash', 'upi'] as const)"
            :key="m"
            class="flex cursor-pointer items-center gap-3 rounded-control border px-3.5 py-2.5"
            :class="method === m ? 'border-primary bg-primary-tint' : 'border-line'"
          >
            <input v-model="method" type="radio" name="method" :value="m" class="accent-primary" />
            <span class="font-semibold">{{ m === 'cash' ? 'Cash' : 'UPI' }}</span>
          </label>
        </div>
        <span v-if="errors.method" class="mt-1 block text-sm font-medium text-danger">{{ errors.method }}</span>
      </fieldset>

      <label class="mt-4 block">
        <span class="font-semibold">Amount (₹)</span>
        <input v-model="amount" name="amount" :disabled="!!pending" class="field mt-1" inputmode="decimal" autocomplete="off" :aria-invalid="!!errors.amount" aria-describedby="pay-amt-err" />
        <span v-if="errors.amount" id="pay-amt-err" class="mt-1 block text-sm font-medium text-danger">{{ errors.amount }}</span>
      </label>

      <label class="mt-4 block">
        <span class="font-semibold">Note <span class="font-normal text-muted">(optional)</span></span>
        <input v-model="note" class="field mt-1" maxlength="200" :disabled="!!pending" />
      </label>

      <p v-if="formError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="dialog?.close()">Cancel</button>
        <button type="submit" class="btn-primary" :disabled="busy || !!pending">Record payment</button>
      </div>
    </form>
  </dialog>
</template>
