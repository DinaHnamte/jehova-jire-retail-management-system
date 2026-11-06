<script setup lang="ts">
import { computed, ref } from 'vue'
import { parseMoneyInput } from '@/shared/money'
import { assign, listDrivers, retry, type Driver } from './api'
import { friendlyMessage, isStatusConflict } from './errors'
import type { DeliveryDetail } from './api'

const props = defineProps<{ delivery: DeliveryDetail; mode: 'assign' | 'reassign' | 'retry' }>()
const emit = defineEmits<{ done: []; stale: [message: string | null] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const drivers = ref<Driver[]>([])
const driverId = ref('')
const pay = ref('')
const tripPay = ref('0')
const error = ref<string | null>(null)
const busy = ref(false)
const loading = ref(false)

const title = computed(() => (props.mode === 'retry' ? 'Send again' : props.mode === 'reassign' ? 'Change driver' : 'Assign a driver'))
const choices = computed(() => drivers.value.filter((d) => d.is_active))

/** Ruling 7: what's on the delivery wins, else the driver's usual pay. */
function prefill() {
  const d = drivers.value.find((x) => x.id === driverId.value)
  pay.value = props.delivery.driver_pay ?? d?.default_pay ?? ''
}

async function open() {
  error.value = null
  busy.value = false
  tripPay.value = '0'
  driverId.value = props.mode === 'reassign' ? '' : (props.delivery.driver?.active ? props.delivery.driver.id : '')
  pay.value = props.delivery.driver_pay ?? ''
  dialog.value?.showModal()
  loading.value = true
  try {
    drivers.value = await listDrivers()
    if (driverId.value && !choices.value.some((d) => d.id === driverId.value)) driverId.value = ''
    prefill()
  } catch (e) {
    error.value = friendlyMessage(e)
  } finally {
    loading.value = false
  }
}
defineExpose({ open })

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function submit() {
  if (busy.value) return
  error.value = null
  if (!driverId.value) {
    error.value = 'Choose a driver.'
    return
  }
  const body: { driver_id: string; driver_pay?: string; trip_pay?: string } = { driver_id: driverId.value }
  if (pay.value.trim()) {
    const p = parseMoneyInput(pay.value)
    if (p === null) {
      error.value = "Enter the driver's pay like 30 or 29.50."
      return
    }
    body.driver_pay = p
  }
  if (props.mode === 'retry') {
    const t = parseMoneyInput(tripPay.value.trim() || '0')
    if (t === null) {
      error.value = 'Enter the pay for the failed trip like 30 or 0.'
      return
    }
    body.trip_pay = t
  }
  busy.value = true
  try {
    if (props.mode === 'retry') await retry(props.delivery.id, body as { driver_id: string })
    else await assign(props.delivery.id, body)
    dialog.value?.close()
    emit('done')
  } catch (e) {
    if (isStatusConflict(e)) {
      dialog.value?.close()
      emit('stale', e.message)
    } else error.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="assign-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="submit">
      <h2 id="assign-title" class="text-xl font-bold">{{ title }}</h2>
      <label class="mt-4 block">
        <span class="font-semibold">Driver</span>
        <select v-model="driverId" class="field mt-1" :disabled="loading || busy" @change="prefill">
          <option value="" disabled>{{ loading ? 'Loading…' : 'Choose a driver' }}</option>
          <option v-for="d in choices" :key="d.id" :value="d.id">{{ d.display_name }}</option>
        </select>
      </label>
      <p v-if="!loading && choices.length === 0 && !error" class="mt-2 text-sm text-muted">No active drivers. Add one under Users.</p>
      <label class="mt-4 block">
        <span class="font-semibold">Driver's pay</span>
        <input v-model="pay" class="field mt-1" inputmode="decimal" autocomplete="off" placeholder="0" :disabled="busy" />
      </label>
      <label v-if="mode === 'retry'" class="mt-4 block">
        <span class="font-semibold">Pay for the failed trip</span>
        <input v-model="tripPay" class="field mt-1" inputmode="decimal" autocomplete="off" :disabled="busy" />
      </label>
      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Close</button>
        <button type="submit" class="btn-primary" :disabled="busy || loading">{{ mode === 'retry' ? 'Send again' : mode === 'reassign' ? 'Change driver' : 'Assign' }}</button>
      </div>
    </form>
  </dialog>
</template>
