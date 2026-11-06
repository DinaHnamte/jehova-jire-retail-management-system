<script setup lang="ts">
import { ref } from 'vue'
import { ApiError } from '@/shared/api/errors'
import { attemptSession } from '@/shared/idempotency'
import { parseMoneyInput } from '@/shared/money'
import { editDelivery, type Collector, type DeliveryDetail, type DriverPayMethod, type EditBody } from './api'
import { friendlyMessage, isStatusConflict } from './errors'

const props = defineProps<{ delivery: DeliveryDetail }>()
const emit = defineEmits<{ done: []; stale: [message: string | null] }>()

const STORE = 'jj.delivery.v1'
const dialog = ref<HTMLDialogElement | null>(null)
const f = ref(blank())
const error = ref<string | null>(null)
const busy = ref(false)
const pending = ref<EditBody | null>(null)

let session = attemptSession<EditBody>(STORE, `edit:${props.delivery.id}`)

function blank() {
  return {
    recipient_name: '',
    recipient_phone: '',
    address_line: '',
    locality: '',
    landmark: '',
    instructions: '',
    fee: '',
    driver_pay: '',
    collector: 'driver' as Collector,
    pay_method: 'retain_from_collection' as DriverPayMethod,
  }
}

function fromDelivery() {
  const d = props.delivery
  f.value = {
    recipient_name: d.recipient_name,
    recipient_phone: d.recipient_phone,
    address_line: d.address_line,
    locality: d.locality,
    landmark: d.landmark ?? '',
    instructions: d.instructions ?? '',
    fee: d.fee,
    driver_pay: d.driver_pay ?? '',
    collector: d.collector,
    pay_method: d.pay_method,
  }
}

function fromBody(b: EditBody) {
  fromDelivery()
  const v = f.value
  if (b.recipient_name !== undefined) v.recipient_name = b.recipient_name
  if (b.recipient_phone !== undefined) v.recipient_phone = b.recipient_phone
  if (b.address_line !== undefined) v.address_line = b.address_line
  if (b.locality !== undefined) v.locality = b.locality
  if (b.landmark !== undefined) v.landmark = b.landmark ?? ''
  if (b.instructions !== undefined) v.instructions = b.instructions ?? ''
  if (b.fee !== undefined) v.fee = b.fee
  if (b.driver_pay !== undefined) v.driver_pay = b.driver_pay ?? ''
  if (b.collector !== undefined) v.collector = b.collector
  if (b.pay_method !== undefined) v.pay_method = b.pay_method
}

function open() {
  error.value = null
  session = attemptSession<EditBody>(STORE, `edit:${props.delivery.id}`)
  pending.value = session.open()
  fromDelivery()
  dialog.value?.showModal()
}
defineExpose({ open })

function retryPending() {
  const b = session.retry()
  if (b) fromBody(b)
  pending.value = null
}
function startAgain() {
  session.startNew()
  pending.value = null
  fromDelivery()
}

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

/** Only what changed; an emptied landmark, instructions or pay is sent as null (cleared). */
function changes(): EditBody | string {
  const d = props.delivery
  const v = f.value
  const b: EditBody = {}
  if (v.recipient_name.trim() !== d.recipient_name) b.recipient_name = v.recipient_name.trim()
  if (v.recipient_phone.trim() !== d.recipient_phone) b.recipient_phone = v.recipient_phone.trim()
  if (v.address_line.trim() !== d.address_line) b.address_line = v.address_line.trim()
  if (v.locality.trim() !== d.locality) b.locality = v.locality.trim()
  if (v.landmark.trim() !== (d.landmark ?? '')) b.landmark = v.landmark.trim() || null
  if (v.instructions.trim() !== (d.instructions ?? '')) b.instructions = v.instructions.trim() || null
  const fee = parseMoneyInput(v.fee)
  if (fee === null) return 'Enter the fee like 50 or 49.50, or 0 for free.'
  if (fee !== d.fee) b.fee = fee
  if (v.driver_pay.trim()) {
    const p = parseMoneyInput(v.driver_pay)
    if (p === null) return "Enter the driver's pay like 30 or 29.50."
    if (p !== d.driver_pay) b.driver_pay = p
  } else if (d.driver_pay !== null) b.driver_pay = null
  const payMethod = v.collector === 'shop' ? 'paid_by_shop' : v.pay_method
  if (v.collector !== d.collector || payMethod !== d.pay_method) {
    b.collector = v.collector
    b.pay_method = payMethod
  }
  return b
}

async function submit() {
  if (busy.value || pending.value) return
  error.value = null
  const body = changes()
  if (typeof body === 'string') {
    error.value = body
    return
  }
  busy.value = true
  try {
    await editDelivery(props.delivery.id, body, session.key(body))
    session.done()
    dialog.value?.close()
    emit('done')
  } catch (e) {
    if (e instanceof ApiError && e.code === 'NETWORK_ERROR') error.value = "No connection. Press Save again — it won't be saved twice."
    else {
      if (e instanceof ApiError && e.status >= 400 && e.status < 500) session.done()
      if (isStatusConflict(e)) {
        dialog.value?.close()
        emit('stale', e.message)
      } else error.value = friendlyMessage(e)
    }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="edit-title"
    class="m-auto max-h-[90dvh] w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="submit">
      <h2 id="edit-title" class="text-xl font-bold">Edit delivery</h2>

      <div v-if="pending" role="alert" class="mt-3 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        <p class="font-semibold">Your last edit may not have gone through.</p>
        <div class="mt-3 grid gap-2 sm:grid-cols-2">
          <button type="button" class="btn-primary" @click="retryPending">Retry it</button>
          <button type="button" class="btn-secondary" @click="startAgain">Start again</button>
        </div>
      </div>

      <fieldset class="mt-4 grid gap-3" :disabled="!!pending || busy">
        <label class="block"><span class="font-semibold">Recipient</span><input v-model="f.recipient_name" class="field mt-1" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Phone</span><input v-model="f.recipient_phone" class="field mt-1" type="tel" inputmode="tel" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Address</span><input v-model="f.address_line" class="field mt-1" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Locality</span><input v-model="f.locality" class="field mt-1" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Landmark <span class="font-normal text-muted">(optional)</span></span><input v-model="f.landmark" class="field mt-1" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Instructions <span class="font-normal text-muted">(optional)</span></span><input v-model="f.instructions" class="field mt-1" autocomplete="off" /></label>
        <div class="grid grid-cols-2 gap-3">
          <label class="block"><span class="font-semibold">Delivery fee</span><input v-model="f.fee" class="field mt-1" inputmode="decimal" autocomplete="off" /></label>
          <label class="block"><span class="font-semibold">Driver's pay</span><input v-model="f.driver_pay" class="field mt-1" inputmode="decimal" autocomplete="off" placeholder="Default" /></label>
        </div>
        <label class="block">
          <span class="font-semibold">Who collects</span>
          <select v-model="f.collector" class="field mt-1">
            <option value="driver">Driver</option>
            <option value="shop">Me (the shop)</option>
          </select>
        </label>
        <label v-if="f.collector === 'driver'" class="block">
          <span class="font-semibold">How the driver is paid</span>
          <select v-model="f.pay_method" class="field mt-1">
            <option value="retain_from_collection">Keeps it from the cash</option>
            <option value="paid_by_shop">I pay them</option>
          </select>
        </label>
      </fieldset>

      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Close</button>
        <button type="submit" class="btn-primary" :disabled="busy || !!pending">Save</button>
      </div>
    </form>
  </dialog>
</template>
