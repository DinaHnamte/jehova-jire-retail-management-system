<script setup lang="ts">
import { computed } from 'vue'
import { theme, toggleTheme } from '@/shared/theme'

withDefaults(defineProps<{ showLabel?: boolean }>(), { showLabel: false })

const dark = computed(() => theme.value === 'dark')
</script>

<template>
  <button
    type="button"
    class="inline-flex min-h-10 items-center gap-2 rounded-control px-2.5 font-semibold transition-colors"
    :aria-pressed="dark"
    :aria-label="showLabel ? undefined : 'Dark mode'"
    :title="dark ? 'Switch to light mode' : 'Switch to dark mode'"
    @click="toggleTheme"
  >
    <!-- moon when dark is on, sun when it is off -->
    <svg viewBox="0 0 24 24" class="size-5 shrink-0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path v-if="dark" d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" />
      <g v-else>
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
      </g>
    </svg>
    <span v-if="showLabel">Dark mode</span>
    <span
      v-if="showLabel"
      class="ml-auto inline-flex h-6 w-10 items-center rounded-full p-0.5 transition-colors"
      :class="dark ? 'bg-primary' : 'bg-nav-muted/45'"
      aria-hidden="true"
    >
      <span class="size-5 rounded-full bg-white shadow transition-transform" :class="dark ? 'translate-x-4' : ''" />
    </span>
  </button>
</template>
