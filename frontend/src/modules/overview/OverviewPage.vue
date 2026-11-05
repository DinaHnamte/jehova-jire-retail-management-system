<script setup lang="ts">
import { computed } from 'vue'
import { useSessionStore } from '@/modules/auth/session.store'

const session = useSessionStore()

// The shop runs on India time whatever the device clock's zone is.
const now = new Date()
const tz = 'Asia/Kolkata'
const weekday = new Intl.DateTimeFormat('en-IN', { weekday: 'long', timeZone: tz }).format(now)
const day = new Intl.DateTimeFormat('en-IN', { day: 'numeric', timeZone: tz }).format(now)
const month = new Intl.DateTimeFormat('en-IN', { month: 'long', year: 'numeric', timeZone: tz }).format(now)

const greeting = computed(() => {
  const hour = Number(new Intl.DateTimeFormat('en-GB', { hour: 'numeric', hourCycle: 'h23', timeZone: tz }).format(now))
  const part = hour < 12 ? 'Good morning' : hour < 17 ? 'Good afternoon' : 'Good evening'
  return `${part}, ${session.user?.display_name ?? ''}`
})
</script>

<template>
  <h1 class="text-2xl font-bold">{{ greeting }}</h1>

  <section aria-label="Today" class="mt-6 flex items-end gap-4 border-b-2 border-line pb-6">
    <span class="figure text-[5.5rem] text-action">{{ day }}</span>
    <span class="pb-1.5 text-lg leading-tight">
      <span class="block font-bold">{{ weekday }}</span>
      <span class="block text-muted">{{ month }}</span>
    </span>
  </section>

  <p class="mt-6 max-w-prose text-muted">
    Sales, stock and deliveries will show here as each part of the shop is set up.
  </p>
</template>
