<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { ApiError } from '@/shared/api/errors'
import { formatRupees, fromPaise, toPaise } from '@/shared/money'
import { formatWhen } from '@/modules/stock/format'
import { publicReceipt, type PublicReceipt } from '@/modules/pos/api'

// Opened by customers from WhatsApp: no session, no app chrome, no cost figures.
const route = useRoute()
const receipt = ref<PublicReceipt | null>(null)
const error = ref<string | null>(null)

onMounted(async () => {
  try {
    receipt.value = await publicReceipt(String(route.params.token))
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) error.value = 'Receipt not found. Check the link you were sent.'
    else if (e instanceof ApiError && e.code === 'RATE_LIMITED') error.value = e.message
    else if (e instanceof ApiError && e.code === 'NETWORK_ERROR') error.value = "Can't load the receipt. Check the internet connection and try again."
    else error.value = 'Something went wrong. Please try again in a moment.'
  }
})

const owed = computed(() => (receipt.value ? toPaise(receipt.value.owed) > 0n : false))
const returned = computed(() => (receipt.value ? receipt.value.returned.reduce((s, l) => s + toPaise(l.amount), 0n) : 0n))
const totalNow = computed(() => (receipt.value ? toPaise(receipt.value.total) - returned.value : 0n))
</script>

<template>
  <main class="mx-auto min-h-dvh max-w-md px-4 py-6">
    <p v-if="error" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
    <p v-else-if="!receipt" class="py-10 text-center text-muted" aria-live="polite">Loading receipt…</p>

    <section v-else class="card p-5 sm:p-6" aria-labelledby="receipt-h">
      <p class="text-lg font-extrabold">{{ receipt.shop_name }}</p>
      <p v-if="receipt.cancelled" role="status" class="mt-3 rounded-control bg-danger px-4 py-2 text-center text-lg font-extrabold tracking-wide text-white">
        CANCELLED
      </p>
      <h1 id="receipt-h" class="mt-3 text-xl font-bold">{{ receipt.number }}</h1>
      <p class="mt-1 text-sm text-muted">{{ formatWhen(receipt.created_at) }}</p>
      <p v-if="receipt.customer_name" class="mt-1 text-sm">{{ receipt.customer_name }}</p>

      <ul class="mt-4 divide-y divide-line border-y border-line">
        <li v-for="(l, i) in receipt.lines" :key="i" class="grid grid-cols-[1fr_auto] gap-x-3 py-2.5">
          <div class="min-w-0">
            <p class="font-semibold">{{ l.name }}</p>
            <p v-if="l.code" class="text-sm text-muted">{{ l.code }}</p>
            <p class="text-sm text-muted">
              {{ l.quantity }} ×
              <s v-if="l.list_price !== l.unit_price" class="mr-1">{{ formatRupees(l.list_price) }}</s>
              {{ formatRupees(l.unit_price) }}
            </p>
          </div>
          <p class="figure self-center">{{ formatRupees(l.net_amount) }}</p>
        </li>
      </ul>

      <div v-if="receipt.returned.length" class="mt-4">
        <h2 class="font-semibold">Returned</h2>
        <ul class="mt-1 divide-y divide-line border-y border-line">
          <li v-for="(l, i) in receipt.returned" :key="i" class="grid grid-cols-[1fr_auto] gap-x-3 py-2.5">
            <div class="min-w-0">
              <p class="font-semibold">{{ l.name }}</p>
              <p v-if="l.code" class="text-sm text-muted">{{ l.code }}</p>
              <p class="text-sm text-muted">{{ l.quantity }} returned</p>
            </div>
            <p class="figure self-center">{{ formatRupees(l.amount) }}</p>
          </li>
        </ul>
      </div>

      <dl class="mt-3 grid grid-cols-[1fr_auto] gap-y-1.5">
        <template v-if="toPaise(receipt.order_discount) > 0n">
          <dt class="text-muted">Subtotal</dt><dd class="figure">{{ formatRupees(receipt.subtotal) }}</dd>
          <dt class="text-muted">Discount</dt><dd class="figure">−{{ formatRupees(receipt.order_discount) }}</dd>
        </template>
        <dt class="font-bold">Total</dt><dd class="figure text-xl">{{ formatRupees(receipt.total) }}</dd>
        <template v-if="returned > 0n">
          <dt class="text-muted">Returned</dt><dd class="figure">−{{ formatRupees(fromPaise(returned)) }}</dd>
          <dt class="font-bold">Now</dt><dd class="figure font-bold">{{ formatRupees(fromPaise(totalNow)) }}</dd>
        </template>
        <dt class="text-muted">Paid</dt><dd class="figure">{{ formatRupees(receipt.paid) }}</dd>
        <dt class="text-muted">Owed</dt><dd class="figure" :class="owed ? 'font-bold text-golden-ink' : ''">{{ formatRupees(receipt.owed) }}</dd>
        <template v-if="toPaise(receipt.refunded) > 0n">
          <dt class="text-muted">Refunded</dt><dd class="figure">{{ formatRupees(receipt.refunded) }}</dd>
        </template>
      </dl>
      <p class="mt-5 text-center text-sm text-muted">Thank you for shopping with us.</p>
    </section>
  </main>
</template>
