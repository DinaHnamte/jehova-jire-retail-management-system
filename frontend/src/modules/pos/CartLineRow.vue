<script setup lang="ts">
import { ref, watch } from 'vue'
import { formatRupees, fromPaise, parseMoneyInput, toPaise } from '@/shared/money'
import { parseQuantity } from '@/modules/stock/totals'
import { lineNet, percentOff, type CartLine } from './cart'
import { useCartStore } from './cart.store'

const props = defineProps<{ line: CartLine; error?: string }>()
const store = useCartStore()

// The store ignores text that doesn't parse, so each input keeps its own text while typing and
// commits on blur / Enter; anything that doesn't parse snaps back to the stored value.
const qtyText = ref('')
const priceText = ref('')
const offText = ref('')
const offError = ref<string | null>(null)

const shownDiscount = (l: CartLine) => (l.lineDiscount === '0.00' ? '' : l.lineDiscount)
watch(() => props.line.quantity, (q) => (qtyText.value = String(q)), { immediate: true })
watch(() => props.line.unitPrice, (p) => (priceText.value = p), { immediate: true })
watch(() => props.line.lineDiscount, () => (offText.value = shownDiscount(props.line)), { immediate: true })

function commitQty() {
  const q = parseQuantity(qtyText.value)
  if (q !== null) store.setQty(props.line.productId, q)
  qtyText.value = String(props.line.quantity)
}
function bump(by: number) {
  store.setQty(props.line.productId, props.line.quantity + by)
}
function commitPrice() {
  if (parseMoneyInput(priceText.value) !== null) store.setPrice(props.line.productId, priceText.value)
  priceText.value = props.line.unitPrice
}

/** "50" or "₹50" is rupees off the line; "10%" is a percentage of quantity × price. */
function commitOff() {
  offError.value = null
  const t = offText.value.trim().replace(/^₹\s*/, '')
  if (t === '') store.setLineDiscount(props.line.productId, '0')
  else if (t.endsWith('%')) {
    const pct = /^\d+(?:\.\d{1,2})?$/.exec(t.slice(0, -1).trim()) ? Number(t.slice(0, -1)) : null
    if (pct === null || pct > 100) offError.value = 'Enter a percentage from 0 to 100, like 10%.'
    else {
      const gross = fromPaise(BigInt(props.line.quantity) * toPaise(props.line.unitPrice))
      store.setLineDiscount(props.line.productId, percentOff(gross, pct))
    }
  } else if (parseMoneyInput(t) !== null) store.setLineDiscount(props.line.productId, t)
  else offError.value = 'Enter rupees like 50 or a percentage like 10%.'
  if (!offError.value) offText.value = shownDiscount(props.line)
}

const cartProblem = () => store.problems.find((p) => p.target === props.line.productId)?.message
</script>

<template>
  <li class="py-3" :class="error ? 'rounded-control bg-danger-tint px-3 -mx-3' : ''">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="figure text-base font-bold tracking-normal">{{ line.code ?? 'No code' }}</p>
        <p class="truncate text-sm text-muted">{{ line.name }}</p>
      </div>
      <button
        type="button"
        class="-mr-2 -mt-1 inline-flex size-11 shrink-0 items-center justify-center rounded-control text-muted hover:bg-canvas hover:text-danger"
        :aria-label="`Remove ${line.code ?? line.name}`"
        @click="store.remove(line.productId)"
      >
        <svg viewBox="0 0 24 24" class="size-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg>
      </button>
    </div>

    <div class="mt-2 flex flex-wrap items-end gap-x-4 gap-y-2">
      <div class="flex items-center">
        <button type="button" class="btn-secondary !min-h-11 !w-11 !px-0 text-xl" :disabled="line.quantity <= 1" aria-label="One less" @click="bump(-1)">−</button>
        <input
          v-model="qtyText"
          class="field !w-14 !px-1 text-center"
          inputmode="numeric"
          autocomplete="off"
          aria-label="Quantity"
          @blur="commitQty"
          @keydown.enter.prevent="commitQty"
        />
        <button type="button" class="btn-secondary !min-h-11 !w-11 !px-0 text-xl" aria-label="One more" @click="bump(1)">+</button>
      </div>

      <label class="block">
        <span class="block text-sm text-muted">
          Price
          <span v-if="line.unitPrice !== line.listPrice" class="line-through">{{ formatRupees(line.listPrice) }}</span>
        </span>
        <input
          v-model="priceText"
          class="field !w-28 !px-3"
          inputmode="decimal"
          autocomplete="off"
          aria-label="Price each"
          @blur="commitPrice"
          @keydown.enter.prevent="commitPrice"
        />
      </label>

      <label class="block">
        <span class="block text-sm text-muted">Off (₹ or %)</span>
        <input
          v-model="offText"
          class="field !w-24 !px-3"
          inputmode="text"
          autocomplete="off"
          placeholder="0"
          aria-label="Discount on this line, rupees or percent"
          :aria-invalid="!!(offError || cartProblem())"
          @blur="commitOff"
          @keydown.enter.prevent="commitOff"
        />
      </label>

      <p class="figure ml-auto pb-3 text-lg">{{ lineNet(line) === null ? '—' : formatRupees(lineNet(line)) }}</p>
    </div>

    <p v-if="offError" role="alert" class="mt-1 text-sm font-medium text-danger">{{ offError }}</p>
    <p v-if="cartProblem()" role="alert" class="mt-1 text-sm font-medium text-danger">{{ cartProblem() }}</p>
    <p v-if="error" role="alert" class="mt-1 text-sm font-medium text-danger">{{ error }}</p>
  </li>
</template>
