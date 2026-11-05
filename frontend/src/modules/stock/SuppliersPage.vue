<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import { createSupplier, listSuppliers, updateSupplier, type Supplier } from './api'

const session = useSessionStore()
const canEdit = computed(() => session.can('purchases.receive'))

const page = ref<Page<Supplier> | null>(null)
const pageNo = ref(1)
const showArchived = ref(false)
const error = ref<string | null>(null)

// One form for add and edit; `editing` is null when adding.
const editing = ref<Supplier | null>(null)
const formOpen = ref(false)
const name = ref('')
const phone = ref('')
const notes = ref('')
const errors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const busy = ref(false)

async function load() {
  error.value = null
  try {
    page.value = await listSuppliers({ includeArchived: showArchived.value, page: pageNo.value })
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

function openForm(s: Supplier | null) {
  editing.value = s
  name.value = s?.name ?? ''
  phone.value = s?.phone ?? ''
  notes.value = s?.notes ?? ''
  errors.value = {}
  formError.value = null
  formOpen.value = true
}

async function save() {
  errors.value = {}
  formError.value = null
  if (!name.value.trim()) {
    errors.value.name = 'Enter the supplier’s name.'
    return
  }
  busy.value = true
  try {
    const body = { name: name.value, phone: phone.value || null, notes: notes.value || null }
    if (editing.value) await updateSupplier(editing.value.id, body)
    else await createSupplier(body)
    formOpen.value = false
    await load()
  } catch (e) {
    const field = e instanceof ApiError ? (e.details as { field?: string })?.field : undefined
    if (field) errors.value[field] = friendlyMessage(e)
    else if (e instanceof ApiError && e.status === 409) errors.value.name = e.message
    else formError.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}

async function toggleArchived(s: Supplier) {
  error.value = null
  try {
    await updateSupplier(s.id, { archived: !s.archived })
    await load()
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}

const lastPage = computed(() => (page.value ? Math.max(1, Math.ceil(page.value.total / page.value.per_page)) : 1))
</script>

<template>
  <RouterLink to="/stock/receipts" class="text-muted hover:text-ink">← Receive goods</RouterLink>
  <div class="mt-2 flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-3xl font-extrabold">Suppliers</h1>
      <p class="mt-1 text-muted">Who the shop buys from.</p>
    </div>
    <button v-if="canEdit && !formOpen" type="button" class="btn-primary" @click="openForm(null)">Add supplier</button>
  </div>

  <form v-if="formOpen" class="card mt-6 grid gap-4 p-5 sm:grid-cols-2 sm:p-6" @submit.prevent="save">
    <h2 class="text-xl font-bold sm:col-span-2">{{ editing ? `Edit ${editing.name}` : 'New supplier' }}</h2>
    <label class="block sm:col-span-2">
      <span class="font-semibold">Name</span>
      <input v-model="name" class="field mt-1" maxlength="120" :aria-invalid="!!errors.name" aria-describedby="sup-name-err" />
      <span v-if="errors.name" id="sup-name-err" class="mt-1 block text-sm font-medium text-danger">{{ errors.name }}</span>
    </label>
    <label class="block">
      <span class="font-semibold">Phone <span class="font-normal text-muted">(optional)</span></span>
      <input v-model="phone" class="field mt-1" type="tel" inputmode="tel" maxlength="20" :aria-invalid="!!errors.phone" />
      <span v-if="errors.phone" class="mt-1 block text-sm font-medium text-danger">{{ errors.phone }}</span>
    </label>
    <label class="block">
      <span class="font-semibold">Notes <span class="font-normal text-muted">(optional)</span></span>
      <input v-model="notes" class="field mt-1" maxlength="500" placeholder="e.g. Delivers on Mondays" :aria-invalid="!!errors.notes" />
      <span v-if="errors.notes" class="mt-1 block text-sm font-medium text-danger">{{ errors.notes }}</span>
    </label>
    <p v-if="formError" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger sm:col-span-2">{{ formError }}</p>
    <div class="flex justify-end gap-2 sm:col-span-2">
      <button type="button" class="btn-secondary" @click="formOpen = false">Cancel</button>
      <button type="submit" class="btn-primary" :disabled="busy">{{ editing ? 'Save supplier' : 'Add supplier' }}</button>
    </div>
  </form>

  <label class="mt-6 inline-flex items-center gap-2 text-muted">
    <input v-model="showArchived" type="checkbox" class="accent-primary" @change="(pageNo = 1), load()" />
    Show archived suppliers
  </label>

  <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <p v-else-if="page && page.total === 0" class="mt-4 text-muted">No suppliers yet. Add the first one to start receiving goods.</p>

  <ul v-else-if="page" class="card mt-4 divide-y divide-line">
    <li v-for="s in page.items" :key="s.id" class="flex flex-wrap items-center justify-between gap-3 px-5 py-4">
      <div>
        <p class="font-bold">{{ s.name }} <Badge v-if="s.archived" tone="neutral">Archived</Badge></p>
        <p v-if="s.phone || s.notes" class="text-sm text-muted">{{ [s.phone, s.notes].filter(Boolean).join('. ') }}</p>
      </div>
      <div v-if="canEdit" class="flex gap-2">
        <button type="button" class="btn-secondary" @click="openForm(s)">Edit</button>
        <button type="button" class="btn-secondary" @click="toggleArchived(s)">{{ s.archived ? 'Restore' : 'Archive' }}</button>
      </div>
    </li>
  </ul>

  <nav v-if="page && page.total > page.per_page" aria-label="Pages" class="mt-4 flex justify-end gap-2">
    <button type="button" class="btn-secondary" :disabled="pageNo <= 1" @click="(pageNo--, load())">Previous</button>
    <button type="button" class="btn-secondary" :disabled="pageNo >= lastPage" @click="(pageNo++, load())">Next</button>
  </nav>
</template>
