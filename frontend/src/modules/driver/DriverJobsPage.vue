<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { myJobs, type MyJobs } from '@/modules/deliveries/api'
import { friendlyMessage } from '@/modules/deliveries/errors'
import { statusLabel } from '@/modules/deliveries/labels'
import { formatRupees } from '@/shared/money'
import { collectLabel } from './actions'

const jobs = ref<MyJobs | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)

async function load() {
  if (loading.value) return
  loading.value = true
  try {
    jobs.value = await myJobs()
    error.value = null
  } catch (e) {
    error.value = friendlyMessage(e)
  } finally {
    loading.value = false
  }
}

const onVisible = () => {
  if (document.visibilityState === 'visible') void load()
}
onMounted(() => {
  void load()
  document.addEventListener('visibilitychange', onVisible)
})
onBeforeUnmount(() => document.removeEventListener('visibilitychange', onVisible))

const shortDate = (iso: string) => new Date(iso).toLocaleDateString('en-IN', { day: 'numeric', month: 'short' })
</script>

<template>
  <div>
    <div class="flex items-center justify-between gap-3">
      <h1 class="display text-2xl font-extrabold">My deliveries</h1>
      <button type="button" class="btn-secondary min-h-12" :disabled="loading" @click="load">Refresh</button>
    </div>

    <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
    <p v-if="!jobs && !error" class="mt-6 text-muted">Loading…</p>

    <template v-if="jobs">
      <p v-if="!jobs.active.length" class="mt-6 rounded-card border border-line bg-surface p-5 text-lg text-muted">Nothing to deliver right now.</p>
      <ul v-else class="mt-4 space-y-3">
        <li v-for="j in jobs.active" :key="j.id">
          <RouterLink :to="`/driver/deliveries/${j.id}`" class="card block min-h-12 p-4 active:bg-ink/5">
            <div class="flex items-start justify-between gap-3">
              <p class="display text-2xl leading-tight font-extrabold">{{ j.locality }}</p>
              <span class="shrink-0 rounded-control bg-golden-tint px-2 py-1 text-sm font-bold text-golden-ink">{{ statusLabel(j.status) }}</span>
            </div>
            <p class="mt-1 text-muted">{{ j.recipient_name }} · {{ j.order_number }}</p>
            <p class="mt-3 text-lg font-bold">
              {{ collectLabel(j) }}
            </p>
          </RouterLink>
        </li>
      </ul>

      <h2 class="display mt-8 text-xl font-extrabold">History</h2>
      <p v-if="!jobs.history.length" class="mt-2 text-muted">No finished deliveries yet.</p>
      <ul v-else class="mt-2 divide-y divide-line rounded-card border border-line bg-surface">
        <li v-for="h in jobs.history" :key="h.id" class="flex min-h-12 items-center justify-between gap-3 px-4 py-3">
          <div class="min-w-0">
            <p class="truncate font-semibold">{{ h.locality }}</p>
            <p class="text-sm text-muted">{{ h.order_number }} · {{ shortDate(h.at) }}</p>
          </div>
          <div class="shrink-0 text-right">
            <p class="font-semibold">{{ formatRupees(h.amount) }}</p>
            <p class="text-sm text-muted">{{ statusLabel(h.status) }}</p>
          </div>
        </li>
      </ul>
    </template>
  </div>
</template>
