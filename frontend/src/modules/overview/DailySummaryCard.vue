<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import { dailySummary, type DailySummary } from '@/modules/pos/api'
import { todayIst } from '@/modules/sales/format'
import { formatDay } from '@/modules/stock/format'

const today = todayIst()
const date = ref(today)
const data = ref<DailySummary | null>(null)
const error = ref<string | null>(null)
let seq = 0

async function load() {
  const mine = ++seq
  error.value = null
  try {
    const d = await dailySummary(date.value)
    if (mine === seq) data.value = d
  } catch (e) {
    if (mine === seq) error.value = friendlyMessage(e)
  }
}
onMounted(load)

function onDate(e: Event) {
  const v = (e.target as HTMLInputElement).value
  if (!v) return
  date.value = v
  load()
}

const figures = computed(() => {
  const d = data.value
  if (!d) return []
  return [
    { label: 'Sales', value: String(d.sales_count), how: 'Number of sales rung up that day, not counting cancelled ones.' },
    { label: 'Sales total', value: formatRupees(d.sales_total), how: 'Add up the total of every sale that day, after discounts. Equals paid on these sales plus new amount owed.' },
    ...(toPaise(d.delivery_fees) > 0n
      ? [{ label: 'of which delivery fees', value: formatRupees(d.delivery_fees), how: 'Delivery fees charged on that day\'s sales. Already inside the sales total.' }]
      : []),
    { label: 'Discounts given', value: formatRupees(d.discounts), how: 'Usual prices of everything sold, minus what customers were charged.' },
    ...(toPaise(d.fee_changes) !== 0n
      ? [{ label: 'Delivery fee changes', value: formatRupees(d.fee_changes), how: 'Delivery fees raised (plus) or lowered (minus) that day on existing orders. Changes what customers owe.' }]
      : []),
    { label: 'Paid on these sales', value: formatRupees(d.paid_on_these_sales), how: "Cash and UPI taken that day against that day's sales." },
    { label: 'New amount owed', value: formatRupees(d.new_owed), how: "That day's sales total minus payments made against those sales." },
    { label: 'Cash received', value: formatRupees(d.cash_received), how: 'All cash taken that day, including payments on older sales.' },
    { label: 'UPI received', value: formatRupees(d.upi_received), how: 'All UPI taken that day, including payments on older sales.' },
    { label: 'Received on older sales', value: formatRupees(d.received_on_older_sales), how: 'Cash and UPI taken that day against sales from earlier days. Cash plus UPI equals this plus paid on these sales.' },
    { label: 'Owed by all customers', value: formatRupees(d.total_owed), how: 'What every customer owes right now, whatever the date picked.' },
  ]
})

const undone = computed(() => {
  const d = data.value
  if (!d) return []
  const items = [
    { label: 'Sales cancelled', value: String(d.cancelled_count), zero: d.cancelled_count === 0, how: 'Number of sales cancelled that day.' },
    { label: 'Cancelled total', value: formatRupees(d.cancelled_total), zero: toPaise(d.cancelled_total) === 0n, how: 'Add up the total of every sale cancelled that day.' },
    { label: 'Returned', value: formatRupees(d.returns_total), zero: toPaise(d.returns_total) === 0n, how: 'Value of the items customers brought back that day, at the price they paid.' },
    { label: 'Refunded in cash', value: formatRupees(d.refunds_cash), zero: toPaise(d.refunds_cash) === 0n, how: 'Cash handed back that day for cancels and returns.' },
    { label: 'Refunded by UPI', value: formatRupees(d.refunds_upi), zero: toPaise(d.refunds_upi) === 0n, how: 'UPI sent back that day for cancels and returns.' },
    { label: 'Owed reduced', value: formatRupees(d.owed_reduced), zero: toPaise(d.owed_reduced) === 0n, how: 'How much less customers owe because of cancels and returns.' },
  ]
  return items.some((i) => !i.zero) ? items : []
})
</script>

<template>
  <section class="card mt-6 p-5 sm:p-6" aria-labelledby="summary-h">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 id="summary-h" class="text-2xl font-extrabold">{{ date === today ? 'Today' : formatDay(date) }}</h2>
        <p v-if="date === today" class="text-sm text-muted">{{ formatDay(date) }}</p>
      </div>
      <label class="text-sm text-muted">
        Date
        <input type="date" class="field mt-1 block" :value="date" :max="today" @change="onDate" />
      </label>
    </div>

    <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">
      {{ error }} <button type="button" class="link ml-1" @click="load">Try again</button>
    </p>
    <p v-else-if="!data" class="mt-4 text-muted" aria-live="polite">Loading…</p>
    <dl v-else class="mt-4 grid gap-3 sm:grid-cols-2">
      <div v-for="f in figures" :key="f.label" class="rounded-control bg-canvas px-4 py-3">
        <dt class="text-sm font-semibold text-muted">{{ f.label }}</dt>
        <dd class="figure mt-1 text-2xl">{{ f.value }}</dd>
        <dd class="mt-1 text-xs text-muted">{{ f.how }}</dd>
      </div>
    </dl>

    <div v-if="undone.length" class="mt-6" role="group" aria-labelledby="undone-h">
      <h3 id="undone-h" class="text-lg font-bold">{{ date === today ? 'Undone today' : 'Undone that day' }}</h3>
      <dl class="mt-3 grid gap-3 sm:grid-cols-2">
        <div v-for="f in undone" :key="f.label" class="rounded-control bg-canvas px-4 py-3">
          <dt class="text-sm font-semibold text-muted">{{ f.label }}</dt>
          <dd class="figure mt-1 text-2xl">{{ f.value }}</dd>
          <dd class="mt-1 text-xs text-muted">{{ f.how }}</dd>
        </div>
      </dl>
    </div>
  </section>
</template>
