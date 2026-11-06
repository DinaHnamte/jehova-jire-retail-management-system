<script setup lang="ts">
import { computed, ref } from 'vue'
import { parseMoneyInput } from '@/shared/money'
import { attemptSession } from '@/shared/idempotency'
import { ApiError } from '@/shared/api/errors'
import { checkIn, type CheckInBody, type DeliveryItem } from './api'
import { friendlyMessage, isStatusConflict } from './errors'

const props = defineProps<{ deliveryId: string; items: DeliveryItem[] }>()
const emit = defineEmits<{ done: []; stale: [message: string | null] }>()

const STORE = 'jj.delivery.v1'
const dialog = ref<HTMLDialogElement | null>(null)
const dispositions = ref<Record<string, 'sellable' | 'damaged'>>({})
const tripPay = ref('0')
const error = ref<string | null>(null)
const busy = ref(false)
const pending = ref<CheckInBody | null>(null)

let session = attemptSession<CheckInBody>(STORE, `check-in:${props.deliveryId}`)

function fill(body: CheckInBody | null) {
  dispositions.value = Object.fromEntries(props.items.map((i) => [i.order_line_id, 'sellable' as const]))
  tripPay.value = '0'
  if (body) {
    for (const l of body.lines) dispositions.value[l.order_line_id] = l.disposition
    tripPay.value = body.trip_pay ?? '0'
  }
}

function open() {
  error.value = null
  session = attemptSession<CheckInBody>(STORE, `check-in:${props.deliveryId}`)
  pending.value = session.open()
  fill(null)
  dialog.value?.showModal()
}
defineExpose({ open })

function retryPending() {
  fill(session.retry())
  pending.value = null
}
function startAgain() {
  session.startNew()
  pending.value = null
  fill(null)
}

const ready = computed(() => !busy.value && !pending.value)
function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function submit() {
  if (!ready.value) return
  error.value = null
  const t = parseMoneyInput(tripPay.value.trim() || '0')
  if (t === null) {
    error.value = 'Enter the pay for the failed trip like 30 or 0.'
    return
  }
  const body: CheckInBody = {
    lines: props.items.map((i) => ({ order_line_id: i.order_line_id, disposition: dispositions.value[i.order_line_id] ?? 'sellable' })),
    trip_pay: t,
  }
  busy.value = true
  try {
    await checkIn(props.deliveryId, body, session.key(body))
    session.done()
    dialog.value?.close()
    emit('done')
  } catch (e) {
    if (e instanceof ApiError && e.code === 'NETWORK_ERROR') error.value = "No connection. Press Check in again — it won't happen twice."
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
    aria-labelledby="checkin-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="submit">
      <h2 id="checkin-title" class="text-xl font-bold">Check the goods back in</h2>
      <p class="mt-2 text-muted">The sale is cancelled. Sellable items go back on the shelf; damaged ones are written off.</p>

      <div v-if="pending" role="alert" class="mt-3 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        <p class="font-semibold">Your last check-in may not have gone through.</p>
        <div class="mt-3 grid gap-2 sm:grid-cols-2">
          <button type="button" class="btn-primary" @click="retryPending">Retry it</button>
          <button type="button" class="btn-secondary" @click="startAgain">Start again</button>
        </div>
      </div>

      <ul class="mt-4 divide-y divide-line border-y border-line">
        <li v-for="i in items" :key="i.order_line_id" class="py-2.5">
          <p class="font-semibold">{{ i.quantity }} × {{ i.name }}</p>
          <div class="mt-2 grid grid-cols-2 gap-2" role="group" :aria-label="`${i.name} condition`">
            <button
              type="button"
              class="min-h-11 rounded-control border px-3 font-semibold"
              :class="dispositions[i.order_line_id] !== 'damaged' ? 'border-primary bg-primary-tint text-primary-ink' : 'border-line text-muted'"
              :aria-pressed="dispositions[i.order_line_id] !== 'damaged'"
              :disabled="!!pending || busy"
              @click="dispositions[i.order_line_id] = 'sellable'"
            >Sellable</button>
            <button
              type="button"
              class="min-h-11 rounded-control border px-3 font-semibold"
              :class="dispositions[i.order_line_id] === 'damaged' ? 'border-danger bg-danger-tint text-danger' : 'border-line text-muted'"
              :aria-pressed="dispositions[i.order_line_id] === 'damaged'"
              :disabled="!!pending || busy"
              @click="dispositions[i.order_line_id] = 'damaged'"
            >Damaged</button>
          </div>
        </li>
      </ul>

      <label class="mt-4 block">
        <span class="font-semibold">Pay for the failed trip</span>
        <input v-model="tripPay" class="field mt-1" inputmode="decimal" autocomplete="off" :disabled="!!pending || busy" />
      </label>

      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Close</button>
        <button type="submit" class="btn-primary" :disabled="!ready">Check in</button>
      </div>
    </form>
  </dialog>
</template>
