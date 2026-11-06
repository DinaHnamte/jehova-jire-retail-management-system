<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
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
    { label: 'Sales total', value: formatRupees(d.sales_total), how: 'Add up the total of every sale that day, after discounts.' },
    { label: 'Discounts given', value: formatRupees(d.discounts), how: 'Usual prices of everything sold, minus what customers were charged.' },
    { label: 'Cash received', value: formatRupees(d.cash_received), how: 'All cash taken that day, including payments on older sales.' },
    { label: 'UPI received', value: formatRupees(d.upi_received), how: 'All UPI taken that day, including payments on older sales.' },
    { label: 'New amount owed', value: formatRupees(d.new_owed), how: "That day's sales total minus payments made against those sales." },
    { label: 'Owed by all customers', value: formatRupees(d.total_owed), how: 'What every customer owes right now, whatever the date picked.' },
  ]
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
  </section>
</template>
