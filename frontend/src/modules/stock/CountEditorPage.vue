<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import Badge from '@/shared/ui/Badge.vue'
import type { SearchHit } from '@/modules/products/api'
import ProductPicker from './ProductPicker.vue'
import { deleteCount, getCount, getProductStock, listMovements, postCount, putCountLines, type StockCount } from './api'
import { formatWhen, signed } from './format'
import { countDifference, fieldToLineIndex, parseQuantity, reviewTone } from './totals'

interface Line {
  product_id: string
  code: string | null
  name: string
  counted: string
  unit_cost: string
  on_hand: number
  first_count: boolean
}

const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const id = route.params.id as string

const count = ref<StockCount | null>(null)
const lines = ref<Line[]>([])
const reviewing = ref(false)
const lineErrors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const saved = ref(false)
const busy = ref(false)
const postDialog = ref<HTMLDialogElement | null>(null)

const editable = computed(() => count.value?.status === 'draft' && session.can('inventory.count'))
const changedCount = computed(() => count.value?.lines.filter((l) => l.changed_since_start).length ?? 0)

function fromCount(c: StockCount) {
  count.value = c
  lines.value = c.lines.map((l) => ({
    product_id: l.product_id,
    code: l.code,
    name: l.name,
    counted: String(l.counted),
    unit_cost: l.unit_cost ?? '',
    on_hand: l.on_hand_now,
    first_count: l.first_count,
  }))
}

onMounted(async () => {
  try {
    fromCount(await getCount(id))
  } catch (e) {
    formError.value = friendlyMessage(e)
  }
})

async function add(h: SearchHit) {
  const line: Line = { product_id: h.id, code: h.code, name: h.name, counted: '', unit_cost: '', on_hand: 0, first_count: false }
  lines.value.push(line)
  saved.value = false
  await nextTick()
  document.getElementById(`counted-${lines.value.length - 1}`)?.focus()
  try {
    const stock = await getProductStock(h.id)
    const first = stock.on_hand === 0 && (await listMovements(h.id, 1, 1)).total === 0
    const l = lines.value.find((x) => x.product_id === h.id)
    if (l) Object.assign(l, { on_hand: stock.on_hand, first_count: first })
  } catch {
    // Saving the count fills these in from the server.
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
    reviewing.value = false
    lineErrors.value = { [`${loc.index}.${loc.key}`]: friendlyMessage(e) }
    await nextTick()
    const el = document.getElementById(`${loc.key === 'unit_cost' ? 'cost' : loc.key === 'counted' ? 'counted' : 'line'}-${loc.index}`)
    el?.scrollIntoView({ block: 'center' })
    el?.focus()
  } else formError.value = friendlyMessage(e)
}

async function save(): Promise<boolean> {
  lineErrors.value = {}
  formError.value = null
  const body = []
  for (const [i, l] of lines.value.entries()) {
    const n = parseQuantity(l.counted, 0)
    if (n === null) {
      await showError(new ApiError(422, 'VALIDATION_FAILED', 'Enter how many you counted (0 if none).', { field: `lines[${i}].counted` }))
      return false
    }
    body.push({ product_id: l.product_id, counted: n, unit_cost: l.first_count && l.unit_cost.trim() ? l.unit_cost : null })
  }
  busy.value = true
  try {
    fromCount(await putCountLines(id, body))
    saved.value = true
    return true
  } catch (e) {
    await showError(e)
    return false
  } finally {
    busy.value = false
  }
}

async function review() {
  if (!lines.value.length) {
    formError.value = 'Add at least one product before reviewing.'
    return
  }
  if (await save()) {
    reviewing.value = true
    window.scrollTo({ top: 0 })
  }
}

async function doPost() {
  busy.value = true
  try {
    fromCount(await postCount(id))
    postDialog.value?.close()
    reviewing.value = true
  } catch (e) {
    postDialog.value?.close()
    await showError(e)
  } finally {
    busy.value = false
  }
}

async function discard() {
  if (!window.confirm('Delete this draft count?')) return
  try {
    await deleteCount(id)
    router.push('/stock/counts')
  } catch (e) {
    formError.value = friendlyMessage(e)
  }
}

const toneClass = { sage: 'text-sage-ink', danger: 'text-danger', neutral: 'text-muted' } as const
</script>

<template>
  <RouterLink to="/stock/counts" class="text-muted hover:text-ink">← Stock counts</RouterLink>

  <p v-if="!count && formError" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>
  <p v-else-if="!count" class="mt-6 text-muted">Loading count…</p>

  <template v-else>
    <h1 class="mt-2 text-3xl font-extrabold">{{ count.note || 'Stock count' }}</h1>
    <p class="mt-1 flex flex-wrap items-center gap-2 text-muted">
      <Badge v-if="count.status === 'draft'" tone="golden">Draft</Badge>
      <Badge v-else tone="sage">Posted</Badge>
      <span v-if="count.status === 'posted'">{{ formatWhen(count.posted_at!) }} by {{ count.posted_by_name }}</span>
      <span v-else>Started {{ formatWhen(count.created_at) }}</span>
    </p>

    <!-- Entering counts -->
    <section v-if="editable && !reviewing" class="card mt-6 p-5 sm:p-6" aria-labelledby="count-h">
      <h2 id="count-h" class="text-xl font-bold">What’s on the shelf</h2>
      <p v-if="!lines.length" class="mt-2 text-muted">Find each product by its code and enter how many you count.</p>
      <ul v-else class="mt-3 divide-y divide-line">
        <li
          v-for="(l, i) in lines"
          :id="`line-${i}`"
          :key="l.product_id"
          tabindex="-1"
          class="grid grid-cols-2 items-start gap-3 py-4 sm:grid-cols-[1fr_8rem_10rem_auto]"
        >
          <div class="col-span-2 sm:col-span-1">
            <p class="figure text-lg tracking-normal">{{ l.code ?? 'No code' }}</p>
            <p class="text-sm text-muted">{{ l.name }}</p>
            <p v-if="err(i, 'product_id')" class="mt-1 text-sm font-medium text-danger">{{ err(i, 'product_id') }}</p>
          </div>
          <label class="block">
            <span class="text-sm text-muted">Counted</span>
            <input
              :id="`counted-${i}`"
              v-model="l.counted"
              class="field mt-1"
              inputmode="numeric"
              autocomplete="off"
              :aria-invalid="!!err(i, 'counted')"
              @input="saved = false"
            />
            <span v-if="err(i, 'counted')" class="mt-1 block text-sm font-medium text-danger">{{ err(i, 'counted') }}</span>
          </label>
          <label v-if="l.first_count" class="block">
            <span class="text-sm text-muted">Cost each (if known)</span>
            <input
              :id="`cost-${i}`"
              v-model="l.unit_cost"
              class="field mt-1"
              inputmode="decimal"
              autocomplete="off"
              placeholder="₹"
              :aria-invalid="!!err(i, 'unit_cost')"
              @input="saved = false"
            />
            <span v-if="err(i, 'unit_cost')" class="mt-1 block text-sm font-medium text-danger">{{ err(i, 'unit_cost') }}</span>
          </label>
          <p v-else class="self-end pb-3 text-sm text-muted">
            App shows {{ l.on_hand }}<template v-if="countDifference(l.counted, l.on_hand) !== null">,
              <span class="font-bold" :class="toneClass[reviewTone(countDifference(l.counted, l.on_hand)!)]">{{ signed(countDifference(l.counted, l.on_hand)!) }}</span></template>
          </p>
          <button type="button" class="col-span-2 self-center justify-self-end text-sm font-semibold text-danger hover:underline sm:col-span-1" @click="remove(i)">
            Remove
          </button>
        </li>
      </ul>
      <div class="mt-4 border-t border-line pt-4">
        <ProductPicker :exclude="lines.map((l) => l.product_id)" @pick="add" />
      </div>
    </section>

    <!-- Review (and the read-only view of a posted count) -->
    <section v-else class="card mt-6 p-5 sm:p-6" aria-labelledby="review-h">
      <h2 id="review-h" class="text-xl font-bold">{{ count.status === 'posted' ? 'Result' : 'Check the differences' }}</h2>
      <p v-if="count.status === 'draft'" class="mt-1 text-muted">Posting changes the stock to what you counted.</p>
      <p v-if="count.status === 'draft' && changedCount" class="mt-3 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        Stock changed for {{ changedCount }} {{ changedCount === 1 ? 'product' : 'products' }} since you started. Recount those before posting.
      </p>
      <table class="mt-4 w-full text-left">
        <thead class="border-b border-line text-sm text-muted">
          <tr>
            <th scope="col" class="py-2 pr-3 font-semibold">Product</th>
            <th scope="col" class="py-2 pr-3 text-right font-semibold">App</th>
            <th scope="col" class="py-2 pr-3 text-right font-semibold">Counted</th>
            <th scope="col" class="py-2 text-right font-semibold">Change</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="l in count.lines" :key="l.product_id" class="border-b border-line last:border-0">
            <td class="py-3 pr-3">
              <span class="figure text-base tracking-normal">{{ l.code ?? 'No code' }}</span>
              <span class="block text-sm text-muted">{{ l.name }}</span>
              <Badge v-if="l.changed_since_start" tone="golden" class="mt-1">Stock changed since you started. Recount this one.</Badge>
            </td>
            <td class="py-3 pr-3 text-right">{{ l.expected ?? l.on_hand_now }}</td>
            <td class="py-3 pr-3 text-right font-bold">{{ l.counted }}</td>
            <td class="figure py-3 text-right text-lg" :class="toneClass[reviewTone(l.difference)]">{{ signed(l.difference) }}</td>
          </tr>
        </tbody>
      </table>
    </section>

    <p v-if="formError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>

    <div v-if="editable" class="mt-6 flex flex-wrap items-center justify-between gap-3">
      <button type="button" class="text-sm font-semibold text-danger hover:underline" @click="discard">Delete draft</button>
      <div v-if="!reviewing" class="flex flex-wrap items-center gap-2">
        <span v-if="saved" class="text-sm text-sage-ink" role="status">Count saved</span>
        <button type="button" class="btn-secondary" :disabled="busy" @click="save">Save</button>
        <button type="button" class="btn-primary" :disabled="busy" @click="review">Review</button>
      </div>
      <div v-else class="flex flex-wrap gap-2">
        <button type="button" class="btn-secondary" @click="reviewing = false">Back to counting</button>
        <button type="button" class="btn-primary" :disabled="busy" @click="postDialog?.showModal()">Post count</button>
      </div>
    </div>

    <dialog
      ref="postDialog"
      aria-labelledby="post-title"
      class="m-auto w-[min(100%-2rem,26rem)] rounded-card border border-line bg-surface p-5 text-ink shadow-card backdrop:bg-ink/40 sm:p-6"
    >
      <h2 id="post-title" class="text-xl font-bold">Post this count?</h2>
      <p class="mt-2 text-muted">Stock for {{ count.lines.length }} {{ count.lines.length === 1 ? 'product' : 'products' }} will be set to what you counted. This can’t be undone.</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="postDialog?.close()">Go back</button>
        <button type="button" class="btn-primary" :disabled="busy" @click="doPost">Post count</button>
      </div>
    </dialog>
  </template>
</template>
