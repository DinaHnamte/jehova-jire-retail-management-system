<script setup lang="ts">
import Badge from '@/shared/ui/Badge.vue'
import { formatRupees, toPaise } from '@/shared/money'
import { openWhatsApp } from '@/shared/whatsapp'
import type { OrderDetail } from './api'

defineProps<{ order: OrderDetail }>()
defineEmits<{ newSale: [] }>()
</script>

<template>
  <section class="card mx-auto max-w-md p-6 text-center" aria-labelledby="done-h">
    <Badge tone="sage" dot>Sale done</Badge>
    <h1 id="done-h" class="mt-3 text-xl font-bold">{{ order.number }}</h1>
    <p class="figure mt-3 text-5xl">{{ formatRupees(order.total) }}</p>
    <dl class="mt-5 grid grid-cols-2 gap-3 text-left">
      <div class="rounded-control bg-canvas px-4 py-3">
        <dt class="text-sm text-muted">Paid</dt>
        <dd class="figure mt-1 text-xl">{{ formatRupees(order.paid) }}</dd>
      </div>
      <div class="rounded-control px-4 py-3" :class="toPaise(order.owed) > 0n ? 'bg-golden-tint' : 'bg-canvas'">
        <dt class="text-sm text-muted">Owed</dt>
        <dd class="figure mt-1 text-xl" :class="toPaise(order.owed) > 0n ? 'text-golden-ink' : ''">{{ formatRupees(order.owed) }}</dd>
      </div>
    </dl>
    <p v-if="order.customer" class="mt-3 text-sm text-muted">For {{ order.customer.name || order.customer.phone }}</p>
    <div class="mt-6 grid gap-3">
      <RouterLink v-if="order.delivery" :to="`/deliveries/${order.delivery.id}`" class="btn-primary">Assign driver</RouterLink>
      <button v-if="order.whatsapp_url" type="button" :class="order.delivery ? 'btn-secondary' : 'btn-primary'" @click="openWhatsApp(order.whatsapp_url)">Send on WhatsApp</button>
      <button type="button" :class="order.whatsapp_url || order.delivery ? 'btn-secondary' : 'btn-primary'" @click="$emit('newSale')">New sale</button>
    </div>
  </section>
</template>
