<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import { getProduct, type SearchHit } from '@/modules/products/api'
import ProductPicker from './ProductPicker.vue'
import { confirmReceipt, deleteReceipt, getReceipt, putReceiptLines, updateReceipt, type Receipt } from './api'
import { formatDay, formatWhen } from './format'
import { fieldToLineIndex, grandTotal, lineTotal, parseQuantity } from './totals'

interface Line {
  product_id: string
  code: string | null
  name: string
  quantity: string
  unit_cost: string
}

const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const id = route.params.id as string

const receipt = ref<Receipt | null>(null)
const lines = ref<Line[]>([])
const receivedOn = ref('')
const note = ref('')
const lineErrors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const saved = ref(false)
const busy = ref(false)
const confirmDialog = ref<HTMLDialogElement | null>(null)

const editable = computed(() => receipt.value?.status === 'draft' && session.can('purchases.receive'))
const total = computed(() => grandTotal(lines.value))

function fromReceipt(r: Receipt) {
  receipt.value = r
  receivedOn.value = r.received_on
  note.value = r.note ?? ''
  lines.value = r.lines.map((l) => ({
    product_id: l.product_id,
    code: l.code,
    name: l.name,
    quantity: String(l.quantity),
    unit_cost: l.unit_cost,
  }))
}

onMounted(async () => {
  try {
    fromReceipt(await getReceipt(id))
  } catch (e) {
    formError.value = friendlyMessage(e)
  }
})

async function add(h: SearchHit) {
  const line: Line = { product_id: h.id, code: h.code, name: h.name, quantity: '', unit_cost: '' }
  lines.value.push(line)
  saved.value = false
  await nextTick()
  document.getElementById(`qty-${lines.value.length - 1}`)?.focus()
  // Pre-fill the usual cost when the product has one (only people who can see costs get it).
  try {
    const p = await getProduct(h.id)
    const l = lines.value.find((x) => x.product_id === h.id)
    if (l && !l.unit_cost && p.default_cost) l.unit_cost = p.default_cost
  } catch {
    // Typing the cost still works.
  }
}

function remove(i: number) {
  lines.value.splice(i, 1)
  saved.value = false
}

const err = (i: number, key: string) => lineErrors.value[`${i}.${key}`]

async function showError(e: unknown) {
  const loc = e instanceof ApiError ? fieldToLineIndex((e.details as { field?: string })?.field) : null
  if (loc) {
    lineErrors.value = { [`${loc.index}.${loc.key}`]: friendlyMessage(e) }
    await nextTick()
    const el = document.getElementById(`${loc.key === 'quantity' ? 'qty' : loc.key === 'unit_cost' ? 'cost' : 'line'}-${loc.index}`)
    el?.scrollIntoView({ block: 'center' })
    el?.focus()
  } else formError.value = friendlyMessage(e)
}

/** Check locally, then save header and lines. Returns false when something needs fixing. */
async function save(): Promise<boolean> {
  lineErrors.value = {}
  formError.value = null
  const body = []
  for (const [i, l] of lines.value.entries()) {
    const q = parseQuantity(l.quantity)
    if (q === null) {
      await showError(new ApiError(422, 'VALIDATION_FAILED', 'Enter a whole number from 1 to 1,000,000.', { field: `lines[${i}].quantity` }))
      return false
    }
    body.push({ product_id: l.product_id, quantity: q, unit_cost: l.unit_cost })
  }
  busy.value = true
  try {
    await updateReceipt(id, { received_on: receivedOn.value, note: note.value || null })
    fromReceipt(await putReceiptLines(id, body))
    saved.value = true
    return true
  } catch (e) {
    await showError(e)
    return false
  } finally {
    busy.value = false
  }
}

async function askConfirm() {
  if (!lines.value.length) {
    formError.value = 'Add at least one product before confirming.'
    return
  }
  if (await save()) confirmDialog.value?.showModal()
}

async function doConfirm() {
  busy.value = true
  try {
    fromReceipt(await confirmReceipt(id))
    confirmDialog.value?.close()
  } catch (e) {
    confirmDialog.value?.close()
    await showError(e)
  } finally {
    busy.value = false
  }
}

async function discard() {
  if (!window.confirm('Delete this draft receipt?')) return
  try {
    await deleteReceipt(id)
    router.push('/stock/receipts')
  } catch (e) {
    formError.value = friendlyMessage(e)
  }
}
</script>

<template>
  <RouterLink to="/stock/receipts" class="text-muted hover:text-ink">← Receive goods</RouterLink>

  <p v-if="!receipt && formError" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>
  <p v-else-if="!receipt" class="mt-6 text-muted">Loading receipt…</p>

  <template v-else>
    <div class="mt-2 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 class="text-3xl font-extrabold">{{ receipt.supplier.name }}</h1>
        <p class="mt-1 flex flex-wrap items-center gap-2 text-muted">
          <Badge v-if="receipt.status === 'draft'" tone="golden">Draft</Badge>
          <Badge v-else tone="sage">Confirmed</Badge>
          <span v-if="receipt.status === 'confirmed'">
            {{ formatWhen(receipt.confirmed_at!) }} by {{ receipt.confirmed_by_name }}
          </span>
        </p>
      </div>
    </div>

    <section class="card mt-6 grid gap-4 p-5 sm:grid-cols-[12rem_1fr] sm:p-6">
      <label class="block">
        <span class="font-semibold">Date received</span>
        <input v-if="editable" v-model="receivedOn" type="date" class="field mt-1" @input="saved = false" />
        <p v-else class="mt-1">{{ formatDay(receipt.received_on) }}</p>
      </label>
      <label class="block">
        <span class="font-semibold">Note <span v-if="editable" class="font-normal text-muted">(optional)</span></span>
        <input v-if="editable" v-model="note" class="field mt-1" maxlength="500" placeholder="e.g. Bill no. 4411" @input="saved = false" />
        <p v-else class="mt-1">{{ receipt.note || '—' }}</p>
      </label>
    </section>

    <section class="card mt-4 p-5 sm:p-6" aria-labelledby="lines-h">
      <h2 id="lines-h" class="text-xl font-bold">Products</h2>
      <p v-if="!lines.length" class="mt-2 text-muted">
        {{ editable ? 'Find each product by its code and enter how many came and what each cost.' : 'No products.' }}
      </p>

      <ul v-else class="mt-3 divide-y divide-line">
        <li
          v-for="(l, i) in lines"
          :id="`line-${i}`"
          :key="l.product_id"
          tabindex="-1"
          class="grid grid-cols-2 items-start gap-3 py-4 sm:grid-cols-[1fr_7rem_9rem_8rem_auto]"
        >
          <div class="col-span-2 sm:col-span-1">
            <p class="figure text-lg tracking-normal">{{ l.code ?? 'No code' }}</p>
            <p class="text-sm text-muted">{{ l.name }}</p>
            <p v-if="err(i, 'product_id')" class="mt-1 text-sm font-medium text-danger">{{ err(i, 'product_id') }}</p>
          </div>
          <label class="block">
            <span class="text-sm text-muted sm:sr-only">Quantity</span>
            <input
              v-if="editable"
              :id="`qty-${i}`"
              v-model="l.quantity"
              class="field mt-1 sm:mt-0"
              inputmode="numeric"
              autocomplete="off"
              :aria-invalid="!!err(i, 'quantity')"
              @input="saved = false"
            />
            <p v-else class="figure text-lg">{{ l.quantity }}</p>
            <span v-if="err(i, 'quantity')" class="mt-1 block text-sm font-medium text-danger">{{ err(i, 'quantity') }}</span>
          </label>
          <label class="block">
            <span class="text-sm text-muted sm:sr-only">Cost each (₹)</span>
            <input
              v-if="editable"
              :id="`cost-${i}`"
              v-model="l.unit_cost"
              class="field mt-1 sm:mt-0"
              inputmode="decimal"
              autocomplete="off"
              :aria-invalid="!!err(i, 'unit_cost')"
              @input="saved = false"
            />
            <p v-else>{{ formatRupees(l.unit_cost) }}</p>
            <span v-if="err(i, 'unit_cost')" class="mt-1 block text-sm font-medium text-danger">{{ err(i, 'unit_cost') }}</span>
          </label>
          <p class="self-center text-right sm:text-right">
            <span class="text-sm text-muted sm:hidden">Line total </span>
            <span class="figure text-lg">{{ formatRupees(lineTotal(l.quantity, l.unit_cost)) }}</span>
          </p>
          <button v-if="editable" type="button" class="self-center justify-self-end text-sm font-semibold text-danger hover:underline" @click="remove(i)">
            Remove
          </button>
        </li>
      </ul>

      <div v-if="editable" class="mt-4 border-t border-line pt-4">
        <ProductPicker :exclude="lines.map((l) => l.product_id)" @pick="add" />
      </div>

      <div class="mt-5 flex items-baseline justify-between border-t border-line pt-4">
        <span class="font-semibold">Total</span>
        <span class="figure text-3xl">{{ formatRupees(receipt.status === 'draft' ? total : receipt.total) }}</span>
      </div>
    </section>

    <p v-if="formError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>

    <div v-if="editable" class="mt-6 flex flex-wrap items-center justify-between gap-3">
      <button type="button" class="text-sm font-semibold text-danger hover:underline" @click="discard">Delete draft</button>
      <div class="flex flex-wrap items-center gap-2">
        <span v-if="saved" class="text-sm text-sage-ink" role="status">Draft saved</span>
        <button type="button" class="btn-secondary" :disabled="busy" @click="save">Save draft</button>
        <button type="button" class="btn-primary" :disabled="busy" @click="askConfirm">Confirm receipt</button>
      </div>
    </div>

    <dialog
      ref="confirmDialog"
      aria-labelledby="confirm-title"
      class="m-auto w-[min(100%-2rem,26rem)] rounded-card border border-line bg-surface p-5 text-ink shadow-card backdrop:bg-ink/40 sm:p-6"
    >
      <h2 id="confirm-title" class="text-xl font-bold">
        Add these {{ lines.length }} {{ lines.length === 1 ? 'product' : 'products' }} to stock?
      </h2>
      <p class="mt-2 text-muted">Total {{ formatRupees(total) }}. This can’t be undone; mistakes are fixed with a stock adjustment.</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="confirmDialog?.close()">Go back</button>
        <button type="button" class="btn-primary" :disabled="busy" @click="doConfirm">Confirm receipt</button>
      </div>
    </dialog>
  </template>
</template>
