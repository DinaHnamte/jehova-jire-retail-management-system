<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import { returnItems, type Disposition, type OrderLine, type PayMethod, type ReturnPreview } from '@/modules/pos/api'
import { clearUndoKey, pendingUndoKey, returnBody } from './undo'

const props = defineProps<{ orderId: string; number: string; lines: OrderLine[] }>()
const emit = defineEmits<{ returned: []; stale: [] }>()

interface Row {
  id: string
  name: string
  code: string | null
  left: number
  quantity: number
  disposition: Disposition
}

const dialog = ref<HTMLDialogElement | null>(null)
const rows = ref<Row[]>([])
const method = ref<PayMethod | null>(null)
const note = ref('')
const preview = ref<ReturnPreview | null>(null)
const previewing = ref(false)
const error = ref<string | null>(null)
const errors = reactive<Record<string, string>>({})
const busy = ref(false)
const previewFailed = ref(false)

const scope = computed(() => `return:${props.orderId}`)

function open() {
  rows.value = props.lines
    .filter((l) => l.quantity - l.returned_quantity > 0)
    .map((l) => ({
      id: l.id,
      name: l.product_name,
      code: l.product_code,
      left: l.quantity - l.returned_quantity,
      quantity: 0,
      disposition: 'sellable' as Disposition,
    }))
  method.value = null
  note.value = ''
  preview.value = null
  previewFailed.value = false
  clearErrors()
  dialog.value?.showModal()
}
defineExpose({ open })

function clearErrors() {
  error.value = null
  for (const k of Object.keys(errors)) delete errors[k]
}

function step(r: Row, by: number) {
  r.quantity = Math.min(r.left, Math.max(0, r.quantity + by))
}

const anyQty = computed(() => rows.value.some((r) => r.quantity > 0))
const refund = computed(() => (preview.value ? toPaise(preview.value.refund) : 0n))
const needsMethod = computed(() => refund.value > 0n)
const ready = computed(() => anyQty.value && !!preview.value && !previewing.value && !previewFailed.value)
const owedReduced = computed(() => (preview.value ? toPaise(preview.value.owed_reduced) : 0n))

// The sent lines are the rows with a quantity, in dialog order; this maps a server `lines[i]` back to a row.
function sentRows(): Row[] {
  return rows.value.filter((r) => r.quantity > 0)
}
function bodyFor(withMethod: boolean) {
  return returnBody(
    rows.value.map((r) => ({ order_line_id: r.id, quantity: r.quantity, disposition: r.disposition })),
    withMethod ? method.value : null,
    note.value,
  )
}

// Preview: debounced, stale requests aborted and ignored.
let timer: ReturnType<typeof setTimeout> | undefined
let ctrl: AbortController | null = null
let seq = 0

function cancelPreview() {
  clearTimeout(timer)
  ctrl?.abort()
  ctrl = null
  seq++
}

watch(
  () => rows.value.map((r) => `${r.id}:${r.quantity}:${r.disposition}`).join('|'),
  () => {
    cancelPreview()
    clearErrors()
    preview.value = null
    previewing.value = false
    previewFailed.value = false
    if (!anyQty.value) return
    previewing.value = true
    const mine = seq
    timer = setTimeout(async () => {
      ctrl = new AbortController()
      const signal = ctrl.signal
      try {
        const p = await returnItems(props.orderId, { ...bodyFor(false), preview: true }, undefined, signal)
        if (mine !== seq) return
        preview.value = p
        previewing.value = false
      } catch (e) {
        if (mine !== seq) return
        previewing.value = false
        previewFailed.value = true
        showError(e, true)
      }
    }, 300)
  },
)
onBeforeUnmount(cancelPreview)

function showError(e: unknown, fromPreview = false) {
  if (e instanceof ApiError && e.code === 'NETWORK_ERROR') {
    error.value = fromPreview
      ? "Couldn't work out the refund. Check your connection."
      : "No connection. Press Return items again — it won't return twice."
    return
  }
  if (e instanceof ApiError && (e.code === 'ALREADY_CANCELLED' || e.code === 'HAS_RETURNS')) emit('stale')
  const field = e instanceof ApiError ? (e.details as { field?: string } | null)?.field : undefined
  const m = field?.match(/^lines\[(\d+)\]\.(quantity|order_line_id|disposition)$/)
  const row = m ? sentRows()[Number(m[1])] : undefined
  if (e instanceof ApiError && row) errors[row.id] = e.message
  else if (e instanceof ApiError && field === 'refund_method') errors.method = e.message
  else if (e instanceof ApiError && field === 'note') errors.note = e.message
  else error.value = friendlyMessage(e)
}

function dismiss() {
  if (!busy.value) dialog.value?.close()
}
function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function confirm() {
  if (busy.value) return
  clearErrors()
  if (!anyQty.value) {
    error.value = 'Choose how many to return.'
    return
  }
  if (needsMethod.value && !method.value) {
    errors.method = 'Choose cash or UPI for the refund.'
    return
  }
  const body = bodyFor(needsMethod.value)
  busy.value = true
  try {
    // The same body keeps the same key, so pressing Return items again after a lost response is safe.
    await returnItems(props.orderId, body, pendingUndoKey(scope.value, body))
    clearUndoKey(scope.value)
    cancelPreview()
    dialog.value?.close()
    emit('returned')
  } catch (e) {
    // The server has answered for good, so a fresh attempt gets a fresh key.
    if (e instanceof ApiError && e.status >= 400 && e.status < 500) clearUndoKey(scope.value)
    showError(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="return-title"
    class="m-auto max-h-[calc(100dvh-2rem)] w-[min(100%-2rem,28rem)] overflow-y-auto rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="confirm">
      <h2 id="return-title" class="text-xl font-bold">Return items from {{ number }}</h2>
      <p class="mt-1 text-muted">Pick how many of each come back.</p>

      <ul class="mt-4 divide-y divide-line border-y border-line">
        <li v-for="r in rows" :key="r.id" class="py-3">
          <div class="flex items-center justify-between gap-3">
            <div class="min-w-0">
              <p class="font-semibold">{{ r.name }}</p>
              <p class="text-sm text-muted">{{ r.left }} left to return<template v-if="r.code"> · {{ r.code }}</template></p>
            </div>
            <div class="flex shrink-0 items-center gap-1" role="group" :aria-label="`Quantity of ${r.name}`">
              <button type="button" class="btn-secondary !min-h-11 !min-w-11 !px-0" :disabled="busy || r.quantity <= 0" :aria-label="`One less ${r.name}`" @click="step(r, -1)">−</button>
              <span class="figure w-8 text-center text-lg font-bold" aria-live="polite">{{ r.quantity }}</span>
              <button type="button" class="btn-secondary !min-h-11 !min-w-11 !px-0" :disabled="busy || r.quantity >= r.left" :aria-label="`One more ${r.name}`" @click="step(r, 1)">+</button>
            </div>
          </div>
          <fieldset v-if="r.quantity > 0" class="mt-2 grid grid-cols-2 gap-2" :disabled="busy">
            <legend class="sr-only">Condition of {{ r.name }}</legend>
            <label
              v-for="d in (['sellable', 'damaged'] as const)"
              :key="d"
              class="flex cursor-pointer items-center gap-2 rounded-control border px-3 py-2"
              :class="r.disposition === d ? 'border-primary bg-primary-tint' : 'border-line'"
            >
              <input v-model="r.disposition" type="radio" :name="`disp-${r.id}`" :value="d" class="accent-primary" />
              <span class="font-semibold">{{ d === 'sellable' ? 'Sellable' : 'Damaged' }}</span>
            </label>
          </fieldset>
          <p v-if="errors[r.id]" role="alert" class="mt-1 text-sm font-medium text-danger">{{ errors[r.id] }}</p>
        </li>
      </ul>

      <div v-if="anyQty" class="mt-4 rounded-control bg-golden-tint px-4 py-3 font-semibold text-golden-ink" aria-live="polite">
        <p v-if="!preview">{{ previewing ? 'Working it out…' : '' }}</p>
        <template v-else>
          <p v-if="owedReduced > 0n">Takes {{ formatRupees(preview.owed_reduced) }} off what they owe</p>
          <p v-if="refund > 0n">Refund {{ formatRupees(preview.refund) }}</p>
          <p v-if="owedReduced === 0n && refund === 0n">Nothing to take off or refund.</p>
        </template>
      </div>

      <fieldset v-if="needsMethod || errors.method" class="mt-4" :disabled="busy">
        <legend class="font-semibold">Refund by</legend>
        <div class="mt-2 grid grid-cols-2 gap-2">
          <label
            v-for="m in (['cash', 'upi'] as const)"
            :key="m"
            class="flex cursor-pointer items-center gap-3 rounded-control border px-3.5 py-2.5"
            :class="method === m ? 'border-primary bg-primary-tint' : 'border-line'"
          >
            <input v-model="method" type="radio" name="refund-method" :value="m" class="accent-primary" @change="delete errors.method" />
            <span class="font-semibold">{{ m === 'cash' ? 'Cash' : 'UPI' }}</span>
          </label>
        </div>
        <span v-if="errors.method" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.method }}</span>
      </fieldset>

      <label class="mt-4 block">
        <span class="font-semibold">Note <span class="font-normal text-muted">(optional)</span></span>
        <input v-model="note" class="field mt-1" maxlength="500" autocomplete="off" :disabled="busy" :aria-invalid="!!errors.note" />
        <span v-if="errors.note" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.note }}</span>
      </label>

      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dismiss">Close</button>
        <button type="submit" class="btn-primary" :disabled="busy || !ready">Return items</button>
      </div>
    </form>
  </dialog>
</template>
