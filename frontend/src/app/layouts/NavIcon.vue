<script setup lang="ts">
import type { NavIconName } from '@/app/nav'

defineProps<{ name: NavIconName }>()

// 24×24 stroke icons, drawn for this app.
const paths: Record<NavIconName, string> = {
  // Shop front with an awning.
  overview: 'M4 10v10h16V10M3 4h18l1 5c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2s-2.2-.9-2.2-2c0 1.1-1 2-2.2 2S2 10.1 2 9l1-5Zm7 16v-5h4v5',
  // Cash register: display on top of a keypad drawer.
  pos: 'M7 3h10v6H7V3Zm-2 6h14l2 4v7H3v-7l2-4Zm-2 4h18M8 16.5h.01M12 16.5h.01M16 16.5h.01',
  // A receipt with a torn zig-zag bottom and text lines.
  sales: 'M6 3h12v18l-2-1.5-2 1.5-2-1.5L10 21l-2-1.5L6 21V3Zm3 5h6M9 12h6M9 16h3',
  // A person on an address card.
  customers: 'M3 5h18v14H3V5Zm6 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm-3.5 5.5c0-1.9 1.6-3 3.5-3s3.5 1.1 3.5 3M14.5 9.5H18M14.5 13H18',
  // A box seen at an angle.
  products: 'M4 7.5 12 3l8 4.5v9L12 21l-8-4.5v-9ZM4 7.5 12 12m0 0 8-4.5M12 12v9',
  // Shelves with stacked boxes.
  stock: 'M3 21h18M3 3v18M21 3v18M3 12h18M6 12V8h4v4M12 12V7h5v5M6 21v-5h5v5M13 21v-4h4v4',
  // Two people.
  users: 'M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm-6 9c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.3a3.5 3.5 0 0 1 0 6.4M18 14.3c1.8.9 3 2.9 3 5.7',
}
</script>

<template>
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path :d="paths[name]" />
  </svg>
</template>
