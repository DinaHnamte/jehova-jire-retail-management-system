<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { searchProducts, type SearchHit } from '@/modules/products/api'

const props = defineProps<{ exclude: string[]; label?: string }>()
const emit = defineEmits<{ pick: [product: SearchHit] }>()

const q = ref('')
const results = ref<SearchHit[]>([])
const open = ref(false)
const active = ref(0)
const input = ref<HTMLInputElement | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined
let ctrl: AbortController | null = null

function onInput() {
  clearTimeout(timer)
  timer = setTimeout(search, 250)
}

async function search() {
  ctrl?.abort()
  const text = q.value.trim()
  if (!text) {
    results.value = []
    open.value = false
    return
  }
  ctrl = new AbortController()
  try {
    const r = await searchProducts(text, ctrl.signal)
    results.value = r.results.filter((h) => !h.archived && !props.exclude.includes(h.id)).slice(0, 8)
    active.value = 0
    open.value = true
  } catch {
    // Typing again retries; a failed lookup isn't worth an error here.
  }
}

function choose(h: SearchHit) {
  emit('pick', h)
  q.value = ''
  results.value = []
  open.value = false
  input.value?.focus()
}

function onKey(e: KeyboardEvent) {
  if (!open.value || !results.value.length) return
  if (e.key === 'ArrowDown') active.value = (active.value + 1) % results.value.length
  else if (e.key === 'ArrowUp') active.value = (active.value - 1 + results.value.length) % results.value.length
  else if (e.key === 'Enter') choose(results.value[active.value]!)
  else if (e.key === 'Escape') open.value = false
  else return
  e.preventDefault()
}

onBeforeUnmount(() => {
  clearTimeout(timer)
  ctrl?.abort()
})
</script>

<template>
  <div class="relative">
    <label class="block">
      <span class="font-semibold">{{ label ?? 'Add a product' }}</span>
      <input
        ref="input"
        v-model="q"
        type="search"
        class="field mt-1"
        inputmode="search"
        autocomplete="off"
        autocapitalize="characters"
        spellcheck="false"
        placeholder="Code or name, e.g. 6143-8"
        role="combobox"
        aria-autocomplete="list"
        :aria-expanded="open"
        aria-controls="picker-list"
        @input="onInput"
        @keydown="onKey"
      />
    </label>
    <ul
      v-if="open"
      id="picker-list"
      role="listbox"
      class="absolute inset-x-0 z-20 mt-1 max-h-80 overflow-auto rounded-control border border-line bg-surface py-1 shadow-card"
    >
      <li v-if="!results.length" class="px-4 py-3 text-muted">No products match “{{ q }}”.</li>
      <li
        v-for="(h, i) in results"
        :key="h.id"
        role="option"
        :aria-selected="i === active"
        class="cursor-pointer px-4 py-2.5"
        :class="i === active ? 'bg-primary-tint' : 'hover:bg-canvas'"
        @mousedown.prevent="choose(h)"
      >
        <span class="figure text-base font-bold tracking-normal">{{ h.code ?? 'No code' }}</span>
        <span v-if="h.design_code" class="ml-1 text-sm text-clay-ink">{{ h.design_code }}</span>
        <span class="block text-sm text-muted">{{ h.name }}</span>
      </li>
    </ul>
  </div>
</template>
