<script setup lang="ts">
import { computed, ref } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import { cancelOrder } from '@/modules/pos/api'
import { clearUndoKey, pendingUndoKey } from './undo'

const props = defineProps<{ orderId: string; number: string; cash: string; upi: string }>()
const emit = defineEmits<{ cancelled: []; stale: [] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const reason = ref('')
const error = ref<string | null>(null)
const busy = ref(false)

const handBack = computed(() => {
  const parts: string[] = []
  if (toPaise(props.cash) > 0n) parts.push(`${formatRupees(props.cash)} cash`)
  if (toPaise(props.upi) > 0n) parts.push(`${formatRupees(props.upi)} UPI`)
  return parts.length ? `Hand back ${parts.join(' and ')}.` : 'Nothing was paid.'
})

function open() {
  error.value = null
  dialog.value?.showModal()
}
defineExpose({ open })

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function confirm() {
  error.value = null
  const r = reason.value.trim()
  const body = r ? { reason: r } : {}
  busy.value = true
  try {
    // The same body keeps the same key, so pressing Cancel sale again after a lost response is safe.
    await cancelOrder(props.orderId, body, pendingUndoKey(`cancel:${props.orderId}`, body))
    clearUndoKey(`cancel:${props.orderId}`)
    dialog.value?.close()
    emit('cancelled')
  } catch (e) {
    if (e instanceof ApiError && e.code === 'NETWORK_ERROR')
      error.value = "No connection. Press Cancel sale again — it won't cancel twice."
    else {
      // The server has answered for good, so a fresh attempt gets a fresh key.
      if (e instanceof ApiError && e.status >= 400 && e.status < 500) clearUndoKey(`cancel:${props.orderId}`)
      // The sale changed under us (already cancelled, or returns were taken): refresh the page behind.
      if (e instanceof ApiError && (e.code === 'ALREADY_CANCELLED' || e.code === 'HAS_RETURNS')) emit('stale')
      error.value = friendlyMessage(e)
    }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="cancel-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="confirm">
      <h2 id="cancel-title" class="text-xl font-bold">Cancel {{ number }}?</h2>
      <p class="mt-2 text-muted">The items go back on the shelf and the sale is reversed.</p>
      <p class="mt-3 rounded-control bg-golden-tint px-4 py-3 font-semibold text-golden-ink">{{ handBack }}</p>

      <label class="mt-4 block">
        <span class="font-semibold">Reason <span class="font-normal text-muted">(optional)</span></span>
        <input v-model="reason" class="field mt-1" maxlength="500" autocomplete="off" />
      </label>

      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Keep sale</button>
        <button type="submit" class="btn-primary" :disabled="busy">Cancel sale</button>
      </div>
    </form>
  </dialog>
</template>
