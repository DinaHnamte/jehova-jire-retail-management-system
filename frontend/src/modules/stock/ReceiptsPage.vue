<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import { createReceipt, createSupplier, listReceipts, listSuppliers, type ReceiptSummary, type Supplier } from './api'
import { formatDay } from './format'

const router = useRouter()
const session = useSessionStore()
const canReceive = computed(() => session.can('purchases.receive'))

const page = ref<Page<ReceiptSummary> | null>(null)
const pageNo = ref(1)
const error = ref<string | null>(null)

// "New receipt" asks for the supplier first, with a shortcut to add one.
const starting = ref(false)
const suppliers = ref<Supplier[]>([])
const supplierId = ref('')
const newSupplier = ref('')
const addingSupplier = ref(false)
const startError = ref<string | null>(null)
const busy = ref(false)

async function load() {
  error.value = null
  try {
    page.value = await listReceipts(pageNo.value)
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

async function start() {
  starting.value = true
  startError.value = null
  try {
    suppliers.value = (await listSuppliers({})).items
    addingSupplier.value = suppliers.value.length === 0
  } catch (e) {
    startError.value = friendlyMessage(e)
  }
}

async function begin() {
  startError.value = null
  busy.value = true
  try {
    let id = supplierId.value
    if (addingSupplier.value) {
      if (!newSupplier.value.trim()) {
        startError.value = 'Enter the supplier’s name.'
        return
      }
      id = (await createSupplier({ name: newSupplier.value })).id
    }
    if (!id) {
      startError.value = 'Pick who the goods came from.'
      return
    }
    const r = await createReceipt({ supplier_id: id })
    router.push(`/stock/receipts/${r.id}`)
  } catch (e) {
    startError.value = e instanceof ApiError ? e.message : friendlyMessage(e)
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
      <h1 class="text-3xl font-extrabold">Receive goods</h1>
      <p class="mt-1 text-muted">Record what arrived from a supplier and what it cost.</p>
    </div>
    <div class="flex flex-wrap gap-2">
      <RouterLink to="/stock/suppliers" class="btn-secondary">Suppliers</RouterLink>
      <button v-if="canReceive && !starting" type="button" class="btn-primary" @click="start">New receipt</button>
    </div>
  </div>

  <form v-if="starting" class="card mt-6 p-5 sm:p-6" @submit.prevent="begin">
    <h2 class="text-xl font-bold">Who are the goods from?</h2>
    <template v-if="!addingSupplier">
      <select v-model="supplierId" class="field mt-3" aria-label="Supplier">
        <option value="" disabled>Pick a supplier</option>
        <option v-for="s in suppliers" :key="s.id" :value="s.id">{{ s.name }}</option>
      </select>
      <button type="button" class="link mt-3 inline-block" @click="addingSupplier = true">Add a new supplier</button>
    </template>
    <template v-else>
      <label class="mt-3 block">
        <span class="font-semibold">New supplier’s name</span>
        <input v-model="newSupplier" class="field mt-1" maxlength="120" placeholder="e.g. Superware Distributors" />
      </label>
      <button v-if="suppliers.length" type="button" class="link mt-3 inline-block" @click="addingSupplier = false">Pick an existing supplier</button>
    </template>
    <p v-if="startError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ startError }}</p>
    <div class="mt-5 flex justify-end gap-2">
      <button type="button" class="btn-secondary" @click="starting = false">Cancel</button>
      <button type="submit" class="btn-primary" :disabled="busy">Start receipt</button>
    </div>
  </form>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <p v-else-if="page && page.total === 0" class="mt-6 text-muted">No receipts yet. When goods arrive, start a new receipt.</p>

  <ul v-else-if="page" class="card mt-6 divide-y divide-line">
    <li v-for="r in page.items" :key="r.id" class="relative grid grid-cols-[1fr_auto] gap-x-4 px-5 py-4 hover:bg-canvas/60">
      <div>
        <RouterLink :to="`/stock/receipts/${r.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">
          {{ r.supplier.name }}
        </RouterLink>
        <p class="mt-0.5 flex flex-wrap items-center gap-2 text-sm text-muted">
          {{ formatDay(r.received_on) }}, {{ r.line_count }} {{ r.line_count === 1 ? 'product' : 'products' }}
          <Badge v-if="r.status === 'draft'" tone="golden">Draft</Badge>
        </p>
      </div>
      <span class="figure self-center text-lg">{{ formatRupees(r.total) }}</span>
    </li>
  </ul>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex justify-end gap-2">
    <button type="button" class="btn-secondary" :disabled="pageNo <= 1" @click="(pageNo--, load())">Previous</button>
    <button type="button" class="btn-secondary" :disabled="pageNo >= lastPage" @click="(pageNo++, load())">Next</button>
  </nav>
</template>
