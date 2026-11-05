<script setup lang="ts">
export type BadgeTone = 'primary' | 'golden' | 'sage' | 'clay' | 'danger' | 'dark' | 'neutral'

const props = withDefaults(defineProps<{ tone?: BadgeTone; dot?: boolean }>(), { tone: 'neutral', dot: false })

const tones: Record<BadgeTone, string> = {
  primary: 'bg-primary-tint text-primary-ink',
  golden: 'bg-golden-tint text-golden-ink',
  sage: 'bg-sage-tint text-sage-ink',
  clay: 'bg-clay-tint text-clay-ink',
  danger: 'bg-danger-tint text-danger',
  dark: 'bg-ink text-surface',
  neutral: 'bg-canvas text-muted',
}
const dots: Record<BadgeTone, string> = {
  primary: 'bg-primary',
  golden: 'bg-golden',
  sage: 'bg-sage',
  clay: 'bg-clay',
  danger: 'bg-danger',
  dark: 'bg-surface',
  neutral: 'bg-muted',
}
</script>

<template>
  <span class="inline-flex items-center gap-1.5 whitespace-nowrap rounded-control px-2.5 py-1 text-sm font-semibold" :class="tones[props.tone]">
    <span v-if="dot" class="size-2 rounded-full" :class="dots[props.tone]" aria-hidden="true" />
    <slot />
  </span>
</template>
