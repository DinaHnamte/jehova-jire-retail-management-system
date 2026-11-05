<script setup lang="ts">
import { computed } from 'vue'
import { useSessionStore } from '@/modules/auth/session.store'
import KitchenwareArt from '@/shared/ui/KitchenwareArt.vue'
import Badge, { type BadgeTone } from '@/shared/ui/Badge.vue'

const session = useSessionStore()

// The shop runs on India time whatever the device clock's zone is.
const now = new Date()
const tz = 'Asia/Kolkata'
const weekday = new Intl.DateTimeFormat('en-IN', { weekday: 'long', timeZone: tz }).format(now)
const day = new Intl.DateTimeFormat('en-IN', { day: 'numeric', timeZone: tz }).format(now)
const month = new Intl.DateTimeFormat('en-IN', { month: 'long', timeZone: tz }).format(now)

const greeting = computed(() => {
  const hour = Number(new Intl.DateTimeFormat('en-GB', { hour: 'numeric', hourCycle: 'h23', timeZone: tz }).format(now))
  const part = hour < 12 ? 'Good morning' : hour < 17 ? 'Good afternoon' : 'Good evening'
  const first = session.user?.display_name.split(/\s+/)[0] ?? ''
  return `${part}, ${first}`
})

interface Upcoming {
  title: string
  text: string
  when: string
  whenTone: BadgeTone
  tile: string
  icon: string
}

// Parts of the app still being built, in roadmap order (Architecture.md §20).
const upcoming: Upcoming[] = [
  {
    title: 'Products',
    text: 'Find any item by the code on its box label.',
    when: 'Next',
    whenTone: 'primary',
    tile: 'bg-golden-tint text-golden-ink',
    icon: 'M4 7.5 12 3l8 4.5v9L12 21l-8-4.5v-9ZM4 7.5 12 12m0 0 8-4.5M12 12v9',
  },
  {
    title: 'Stock',
    text: 'Goods received, counts and what is running low.',
    when: 'Coming soon',
    whenTone: 'neutral',
    tile: 'bg-sage-tint text-sage-ink',
    icon: 'M3 21h18M5 21V9l7-5 7 5v12M9 21v-6h6v6',
  },
  {
    title: 'Counter',
    text: 'Ring up sales, take cash or UPI, print receipts.',
    when: 'Coming soon',
    whenTone: 'neutral',
    tile: 'bg-primary-tint text-primary-ink',
    icon: 'M4 6h16v12H4zM4 10h16M8 14h3',
  },
  {
    title: 'Deliveries',
    text: 'Send orders out with a driver and track the cash.',
    when: 'Coming soon',
    whenTone: 'neutral',
    tile: 'bg-clay-tint text-clay-ink',
    icon: 'M3 6h11v10H3zM14 10h4l3 3v3h-7M7 19a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm10 0a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z',
  },
]
</script>

<template>
  <section class="relative overflow-hidden rounded-card border border-line bg-surface p-6 sm:p-8 md:grid md:grid-cols-[1fr_auto] md:items-center md:gap-8">
    <div>
      <h1 class="text-3xl font-extrabold sm:text-4xl">{{ greeting }}</h1>
      <div class="mt-6 flex items-end gap-4">
        <span class="figure text-[5.5rem] text-primary">{{ day }}</span>
        <span class="pb-2 text-lg leading-tight">
          <span class="block font-bold">{{ weekday }}</span>
          <span class="block text-muted">{{ month }}</span>
        </span>
      </div>
    </div>
    <KitchenwareArt class="-mb-2 mt-4 ml-auto w-full max-w-52 justify-self-end text-ink md:mt-0 md:w-80 md:max-w-none" />
  </section>

  <h2 class="mt-10 text-2xl font-extrabold">Getting the shop ready</h2>
  <p class="mt-1 max-w-prose text-muted">These parts of the app are being built. Each one appears here once it's switched on.</p>

  <ul class="mt-5 grid gap-4 sm:grid-cols-2">
    <li v-for="u in upcoming" :key="u.title" class="card flex gap-4 p-5">
      <span class="grid size-14 shrink-0 place-items-center rounded-control" :class="u.tile">
        <svg viewBox="0 0 24 24" class="size-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path :d="u.icon" />
        </svg>
      </span>
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2">
          <h3 class="text-xl font-bold">{{ u.title }}</h3>
          <Badge :tone="u.whenTone">{{ u.when }}</Badge>
        </div>
        <p class="mt-1 text-muted">{{ u.text }}</p>
      </div>
    </li>
  </ul>
</template>
