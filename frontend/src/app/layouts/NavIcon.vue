<script setup lang="ts">
import type { NavIconName } from '@/app/nav'

defineProps<{ name: NavIconName }>()

// 24×24 stroke icons, drawn for this app.
const paths: Record<NavIconName, string> = {
  // Shop front with an awning.
  overview: 'M4 10v10h16V10M3 4h18l1 5c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2S2 10.1 2 9l1-5Zm7 16v-5h4v5',
  // Two people.
  users: 'M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm-6 9c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.3a3.5 3.5 0 0 1 0 6.4M18 14.3c1.8.9 3 2.9 3 5.7',
}
</script>

<template>
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path :d="paths[name]" />
  </svg>
</template>
