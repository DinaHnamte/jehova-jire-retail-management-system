<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { formatRupees, parseMoneyInput } from '@/shared/money'
import { searchProducts, type SearchHit } from '@/modules/products/api'
import type { OrderDetail } from './api'
import { roundDownTo } from './cart'
import { useCartStore } from './cart.store'
import CartLineRow from './CartLineRow.vue'
import ChargeSheet from './ChargeSheet.vue'
import CustomerPicker from './CustomerPicker.vue'
import SaleDone from './SaleDone.vue'

const store = useCartStore()

const q = ref('')
const results = ref<SearchHit[]>([])
const searched = ref(false)
const input = ref<HTMLInputElement | null>(null)
const sheet = ref<InstanceType<typeof ChargeSheet> | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined
let ctrl: AbortController | null = null
let inflight: Promise<void> | null = null

const done = ref<OrderDetail | null>(null)
/** Server complaints about a particular line, keyed by product id. */
const lineErrors = ref<Record<string, string>>({})

function onInput() {
  // Hits belong to the text they were searched for; drop them (and any in-flight search) on change.
  ctrl?.abort()
  results.value = []
  searched.value = false
  clearTimeout(timer)
  timer = setTimeout(search, 120)
}

async function search() {
  timer = undefined
  ctrl?.abort()
  const text = q.value.trim()
  if (!text) {
    results.value = []
    searched.value = false
    return
  }
  const mine = new AbortController()
  ctrl = mine
  const run = (async () => {
    try {
      const r = await searchProducts(text, mine.signal)
      if (mine.signal.aborted) return
      results.value = r.results.filter((h) => !h.archived).slice(0, 8)
      searched.value = true
    } catch {
      // Typing again retries; an aborted or failed lookup isn't worth an error here.
    }
  })()
  inflight = run
  await run
  if (inflight === run) inflight = null
}

const inCart = (id: string) => store.lines.find((l) => l.productId === id)?.quantity ?? 0

function add(h: SearchHit) {
  store.add(h)
}

async function onEnter() {
  if (timer !== undefined) {
    clearTimeout(timer)
    await search()
  } else if (inflight) await inflight
  const top = results.value[0]
  if (!top) return
  add(top)
  q.value = ''
  results.value = []
  searched.value = false
}

// Whole-sale discount
const offText = ref('')
const roundText = ref('')
const discountError = ref<string | null>(null)
watch(
  () => store.orderDiscount,
  (d) => (offText.value = d === '0.00' ? '' : d),
  { immediate: true },
)

function commitOff() {
  discountError.value = null
  if (offText.value.trim() === '') store.setOrderDiscount('0')
  else if (parseMoneyInput(offText.value) !== null) store.setOrderDiscount(offText.value)
  else discountError.value = 'Enter an amount like 50 or 49.50.'
  offText.value = store.orderDiscount === '0.00' ? '' : store.orderDiscount
}

function commitRound() {
  discountError.value = null
  const t = roundText.value.trim()
  if (t === '') return
  const target = parseMoneyInput(t)
  if (target === null) discountError.value = 'Enter the amount to charge, like 1200.'
  else {
    const d = roundDownTo(store.totals.subtotal, target)
    if (d === null) discountError.value = `That is more than the bill (${formatRupees(store.totals.subtotal)}).`
    else {
      store.setOrderDiscount(d)
      roundText.value = ''
    }
  }
}

const orderProblem = () => store.problems.find((p) => p.target === 'order')?.message

function openCharge() {
  if (done.value || !store.valid) return
  lineErrors.value = {}
  sheet.value?.open()
}

function onLineError(productId: string, message: string) {
  lineErrors.value = { [productId]: message }
}

// Any change to the cart makes an old server complaint stale.
watch(() => [store.lines, store.orderDiscount], () => (lineErrors.value = {}), { deep: true })

/** The cart (and its idempotency key) is cleared the moment the sale succeeds, so a reload can't charge it twice. */
function onDone(order: OrderDetail) {
  store.clear()
  done.value = order
}

function newSale() {
  done.value = null
  q.value = ''
  results.value = []
}

function onKey(e: KeyboardEvent) {
  if (done.value) return
  if (e.key === 'F2') {
    e.preventDefault()
    openCharge()
  } else if (e.key === '/' && !e.ctrlKey && !e.metaKey && !e.altKey) {
    const t = e.target as HTMLElement | null
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return
    if (document.querySelector('dialog[open]')) return
    e.preventDefault()
    input.value?.focus()
  }
}

onMounted(() => document.addEventListener('keydown', onKey))
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey)
  clearTimeout(timer)
  ctrl?.abort()
})
</script>

<template>
  <SaleDone v-if="done" :order="done" @new-sale="newSale" />

  <template v-else>
    <h1 class="text-3xl font-extrabold">Sell</h1>

    <div class="relative mt-4">
      <label class="block">
        <span class="font-semibold">Find a product <span class="font-normal text-muted">(press / to search)</span></span>
        <input
          ref="input"
          v-model="q"
          type="search"
          class="field mt-1"
          inputmode="search"
          autocomplete="off"
          autocapitalize="characters"
          spellcheck="false"
          placeholder="Code or name, e.g. 6143-8"
          @input="onInput"
          @keydown.enter.prevent="onEnter"
        />
      </label>
      <ul v-if="searched" class="mt-2 divide-y divide-line rounded-control border border-line bg-surface">
        <li v-if="!results.length" class="px-4 py-3 text-muted">No products match “{{ q }}”.</li>
        <li v-for="(h, i) in results" :key="h.id">
          <button
            type="button"
            class="flex min-h-14 w-full items-center justify-between gap-3 px-4 py-2 text-left"
            :class="i === 0 ? 'bg-primary-tint' : 'hover:bg-canvas'"
            @click="add(h)"
          >
            <span class="min-w-0">
              <span class="figure text-base font-bold tracking-normal">{{ h.code ?? 'No code' }}</span>
              <span v-if="h.design_code" class="ml-1 text-sm text-clay-ink">{{ h.design_code }}</span>
              <span class="block truncate text-sm text-muted">{{ h.name }}</span>
            </span>
            <span class="shrink-0 text-right">
              <span class="figure block text-base">{{ formatRupees(h.selling_price) }}</span>
              <span v-if="inCart(h.id)" class="text-sm font-semibold text-sage-ink">{{ inCart(h.id) }} in cart</span>
            </span>
          </button>
        </li>
      </ul>
    </div>

    <section class="card mt-4 p-4" aria-labelledby="cart-h">
      <h2 id="cart-h" class="text-xl font-bold">Cart</h2>
      <p v-if="!store.lines.length" class="mt-2 text-muted">Search for a product and tap it to add.</p>
      <ul v-else class="mt-1 divide-y divide-line">
        <CartLineRow v-for="l in store.lines" :key="l.productId" :line="l" :error="lineErrors[l.productId]" />
      </ul>

      <div v-if="store.lines.length" class="mt-3 border-t border-line pt-3">
        <div class="flex flex-wrap items-end gap-3">
          <label class="block">
            <span class="block font-semibold">Discount on the sale</span>
            <input
              v-model="offText"
              class="field !w-32"
              inputmode="decimal"
              autocomplete="off"
              placeholder="₹ off"
              aria-label="Rupees off the whole sale"
              :aria-invalid="!!orderProblem()"
              @blur="commitOff"
              @keydown.enter.prevent="commitOff"
            />
          </label>
          <label class="block">
            <span class="block text-sm text-muted">or round to ₹</span>
            <input
              v-model="roundText"
              class="field !w-32"
              inputmode="decimal"
              autocomplete="off"
              placeholder="e.g. 1200"
              aria-label="Round the total down to this amount"
              @blur="commitRound"
              @keydown.enter.prevent="commitRound"
            />
          </label>
        </div>
        <p v-if="discountError" role="alert" class="mt-1 text-sm font-medium text-danger">{{ discountError }}</p>
        <p v-if="orderProblem()" role="alert" class="mt-1 text-sm font-medium text-danger">{{ orderProblem() }}</p>
        <dl class="mt-3 space-y-1 text-right text-muted">
          <div class="flex justify-between"><dt>Subtotal</dt><dd>{{ formatRupees(store.totals.subtotal) }}</dd></div>
          <div v-if="store.orderDiscount !== '0.00'" class="flex justify-between"><dt>Discount</dt><dd>−{{ formatRupees(store.totals.discount) }}</dd></div>
        </dl>
      </div>
    </section>

    <section class="card mt-4 p-4" aria-label="Customer">
      <CustomerPicker />
    </section>

    <div class="sticky bottom-20 z-10 mt-4 md:bottom-4">
      <div class="card flex items-center justify-between gap-3 p-3 pl-4">
        <div>
          <p class="text-sm text-muted">{{ store.count }} item{{ store.count === 1 ? '' : 's' }}</p>
          <p class="figure text-3xl">{{ formatRupees(store.totals.total) }}</p>
        </div>
        <button type="button" class="btn-primary" :disabled="!store.valid" title="F2" @click="openCharge">Charge</button>
      </div>
    </div>

    <ChargeSheet ref="sheet" @done="onDone" @line-error="onLineError" />
  </template>
</template>
