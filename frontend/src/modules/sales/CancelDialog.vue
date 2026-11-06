<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import { cancelOrder, getOrder, type CancelBody } from '@/modules/pos/api'
import { cancelSession } from './undo'

const props = defineProps<{ orderId: string; number: string; cash: string; upi: string }>()
const emit = defineEmits<{ cancelled: []; stale: [] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const reason = ref('')
const error = ref<string | null>(null)
const busy = ref(false)
// An earlier cancel that got no answer: the user must choose to retry it or start again.
const pending = ref<CancelBody | null>(null)
// What was paid, read fresh when the dialog opens, so a payment made elsewhere isn't missed.
const paid = ref<{ cash: string; upi: string } | null>(null)
const checking = ref(false)
const checkFailed = ref(false)
// The sale can no longer be cancelled (already cancelled, or items were returned).
const blocked = ref(false)

let session = cancelSession(props.orderId)
watch(
  () => props.orderId,
  () => (session = cancelSession(props.orderId)),
)

const handBack = computed(() => {
  if (!paid.value) return checking.value ? 'Checking what was paid…' : ''
  const parts: string[] = []
  if (toPaise(paid.value.cash) > 0n) parts.push(`${formatRupees(paid.value.cash)} cash`)
  if (toPaise(paid.value.upi) > 0n) parts.push(`${formatRupees(paid.value.upi)} UPI`)
  return parts.length ? `Hand back ${parts.join(' and ')}.` : 'Nothing was paid.'
})

let check = 0
async function refresh() {
  const mine = ++check
  paid.value = null
  checking.value = true
  checkFailed.value = false
  try {
    const o = await getOrder(props.orderId)
    if (mine !== check) return
    paid.value = o.paid_by_method
    if (o.status === 'cancelled' || o.returns.length > 0) {
      blocked.value = true
      error.value =
        o.status === 'cancelled'
          ? 'This sale was already cancelled.'
          : 'Some items were already returned. Return the rest instead.'
    }
    // The page behind is out of date: refresh it too.
    if (blocked.value || o.paid_by_method.cash !== props.cash || o.paid_by_method.upi !== props.upi) emit('stale')
  } catch (e) {
    if (mine !== check) return
    checkFailed.value = true
    error.value =
      e instanceof ApiError && e.code === 'NETWORK_ERROR'
        ? "Couldn't check what was paid. Check your connection and try again."
        : friendlyMessage(e)
  } finally {
    if (mine === check) checking.value = false
  }
}

function retryCheck() {
  error.value = null
  void refresh()
}

function open() {
  error.value = null
  blocked.value = false
  reason.value = ''
  pending.value = session.open()
  dialog.value?.showModal()
  void refresh()
}
defineExpose({ open })

/** Send the earlier cancel again as it was: same body, same key, so it happens once. */
function retryPending() {
  reason.value = session.retry()?.reason ?? ''
  pending.value = null
}

/** Forget the earlier cancel: this one gets a new key. */
function startAgain() {
  session.startNew()
  pending.value = null
  reason.value = ''
}

const ready = computed(() => !busy.value && !pending.value && !!paid.value && !blocked.value)

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function confirm() {
  if (!ready.value) return
  error.value = null
  const r = reason.value.trim()
  const body: CancelBody = r ? { reason: r } : {}
  busy.value = true
  try {
    // The same body keeps the same key, so pressing Cancel sale again after a lost response is safe.
    await cancelOrder(props.orderId, body, session.key(body))
    session.done()
    dialog.value?.close()
    emit('cancelled')
  } catch (e) {
    if (e instanceof ApiError && e.code === 'NETWORK_ERROR')
      error.value = "No connection. Press Cancel sale again — it won't cancel twice."
    else {
      // The server has answered for good, so a fresh attempt gets a fresh key.
      if (e instanceof ApiError && e.status >= 400 && e.status < 500) session.done()
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

      <div v-if="pending" role="alert" class="mt-3 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        <p class="font-semibold">Your last cancel request may not have gone through.</p>
        <div class="mt-3 grid gap-2 sm:grid-cols-2">
          <button type="button" class="btn-primary" @click="retryPending">Retry it</button>
          <button type="button" class="btn-secondary" @click="startAgain">Start again</button>
        </div>
      </div>

      <p v-if="handBack" class="mt-3 rounded-control bg-golden-tint px-4 py-3 font-semibold text-golden-ink" aria-live="polite">{{ handBack }}</p>

      <label class="mt-4 block">
        <span class="font-semibold">Reason <span class="font-normal text-muted">(optional)</span></span>
        <input v-model="reason" class="field mt-1" maxlength="500" autocomplete="off" :disabled="!!pending || busy" />
      </label>

      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
      <div v-if="checkFailed && !checking" class="mt-2 flex justify-end">
        <button type="button" class="btn-secondary" @click="retryCheck">Try again</button>
      </div>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Keep sale</button>
        <button type="submit" class="btn-primary" :disabled="!ready">Cancel sale</button>
      </div>
    </form>
  </dialog>
</template>
