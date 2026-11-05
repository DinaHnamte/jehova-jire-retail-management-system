<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import { createCount, listCounts, type CountSummary } from './api'
import { formatWhen } from './format'

const router = useRouter()
const session = useSessionStore()
const canCount = computed(() => session.can('inventory.count'))

const page = ref<Page<CountSummary> | null>(null)
const pageNo = ref(1)
const error = ref<string | null>(null)
const starting = ref(false)
const note = ref('')
const busy = ref(false)

async function load() {
  error.value = null
  try {
    page.value = await listCounts(pageNo.value)
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

async function begin() {
  busy.value = true
  error.value = null
  try {
    const c = await createCount(note.value.trim() || null)
    router.push(`/stock/counts/${c.id}`)
  } catch (e) {
    error.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}

const lastPage = computed(() => (page.value ? Math.max(1, Math.ceil(page.value.total / page.value.per_page)) : 1))
</script>

<template>
  <RouterLink to="/stock" class="text-muted hover:text-ink">← Stock</RouterLink>
  <div class="mt-2 flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-3xl font-extrabold">Stock counts</h1>
      <p class="mt-1 max-w-prose text-muted">
        Count what’s on the shelves. A product’s first count sets its opening stock; later counts correct the app to match the shelf.
      </p>
    </div>
    <button v-if="canCount && !starting" type="button" class="btn-primary" @click="starting = true">New count</button>
  </div>

  <form v-if="starting" class="card mt-6 p-5 sm:p-6" @submit.prevent="begin">
    <label class="block">
      <span class="font-semibold">What are you counting? <span class="font-normal text-muted">(optional)</span></span>
      <input v-model="note" class="field mt-1" maxlength="500" placeholder="e.g. Opening count, or Shelf A" />
    </label>
    <div class="mt-5 flex justify-end gap-2">
      <button type="button" class="btn-secondary" @click="starting = false">Cancel</button>
      <button type="submit" class="btn-primary" :disabled="busy">Start count</button>
    </div>
  </form>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <p v-else-if="page && page.total === 0" class="mt-6 text-muted">
    No counts yet. Before the first sale, count everything on the shelves once.
  </p>

  <ul v-else-if="page" class="card mt-6 divide-y divide-line">
    <li v-for="c in page.items" :key="c.id" class="relative px-5 py-4 hover:bg-canvas/60">
      <RouterLink :to="`/stock/counts/${c.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">
        {{ c.note || 'Stock count' }}
      </RouterLink>
      <p class="mt-0.5 flex flex-wrap items-center gap-2 text-sm text-muted">
        {{ formatWhen(c.posted_at ?? c.created_at) }}, {{ c.line_count }} {{ c.line_count === 1 ? 'product' : 'products' }}
        <Badge v-if="c.status === 'draft'" tone="golden">Draft</Badge>
      </p>
    </li>
  </ul>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex justify-end gap-2">
    <button type="button" class="btn-secondary" :disabled="pageNo <= 1" @click="(pageNo--, load())">Previous</button>
    <button type="button" class="btn-secondary" :disabled="pageNo >= lastPage" @click="(pageNo++, load())">Next</button>
  </nav>
</template>
