<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ name: string }>()

const palette = [
  'bg-primary-tint text-primary-ink',
  'bg-golden-tint text-golden-ink',
  'bg-sage-tint text-sage-ink',
  'bg-clay-tint text-clay-ink',
]

const initials = computed(() =>
  props.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join(''),
)

// Same name, same colour, every time.
const tone = computed(() => {
  let h = 0
  for (const c of props.name) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return palette[h % palette.length]
})
</script>

<template>
  <span class="display inline-grid shrink-0 place-items-center rounded-full font-extrabold" :class="tone" aria-hidden="true">
    {{ initials }}
  </span>
</template>
