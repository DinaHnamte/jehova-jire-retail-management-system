<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import Badge from '@/shared/ui/Badge.vue'
import { formatRupees, toPaise } from '@/shared/money'
import { searchCustomers, type CustomerRow } from './api'
import { useCartStore } from './cart.store'

const store = useCartStore()

const q = ref('')
const results = ref<CustomerRow[]>([])
const searched = ref(false)
const newName = ref('')
let timer: ReturnType<typeof setTimeout> | undefined
let ctrl: AbortController | null = null

function onInput() {
  clearTimeout(timer)
  timer = setTimeout(search, 200)
}

async function search() {
  ctrl?.abort()
  const text = q.value.trim()
  if (!text) {
    results.value = []
    searched.value = false
    return
  }
  ctrl = new AbortController()
  try {
    results.value = (await searchCustomers(text, ctrl.signal)).items.slice(0, 6)
    searched.value = true
  } catch {
    // Typing again retries.
  }
}

/** Ten digits when the text is a phone number (an optional 91 or +91 is dropped). */
const phone = computed(() => {
  const d = q.value.replace(/\D/g, '')
  const ten = d.length === 12 && d.startsWith('91') ? d.slice(2) : d
  return /^\d{10}$/.test(ten) ? ten : null
})
const phoneShown = computed(() => (phone.value ? `${phone.value.slice(0, 5)} ${phone.value.slice(5)}` : ''))
const canAdd = computed(() => !!phone.value && searched.value && !results.value.some((c) => c.phone.endsWith(phone.value!)))

function reset() {
  clearTimeout(timer)
  ctrl?.abort()
  q.value = ''
  newName.value = ''
  results.value = []
  searched.value = false
}
function pick(c: CustomerRow) {
  store.setCustomer(c)
  reset()
}
function addNew() {
  if (!phone.value) return
  store.setNewCustomer({ phone: phone.value, name: newName.value.trim() })
  reset()
}

const owes = computed(() => (store.customer as Partial<CustomerRow> | null)?.owes)
const owesMoney = computed(() => !!owes.value && toPaise(owes.value) > 0n)
const fmtPhone = (p: string) => (p.length === 10 ? `${p.slice(0, 5)} ${p.slice(5)}` : p)

onBeforeUnmount(() => {
  clearTimeout(timer)
  ctrl?.abort()
})
</script>

<template>
  <div>
    <div v-if="store.customer || store.newCustomer" class="flex items-center justify-between gap-3 rounded-control border border-line px-4 py-2.5">
      <div class="min-w-0">
        <template v-if="store.customer">
          <p class="truncate font-semibold">{{ store.customer.name || fmtPhone(store.customer.phone) }}</p>
          <p class="flex flex-wrap items-center gap-2 text-sm text-muted">
            <span v-if="store.customer.name">{{ fmtPhone(store.customer.phone) }}</span>
            <Badge v-if="owesMoney" tone="golden">owes {{ formatRupees(owes) }}</Badge>
          </p>
        </template>
        <template v-else-if="store.newCustomer">
          <p class="truncate font-semibold">{{ store.newCustomer.name || fmtPhone(store.newCustomer.phone) }}</p>
          <p class="text-sm text-muted">New customer · {{ fmtPhone(store.newCustomer.phone) }}</p>
        </template>
      </div>
      <button type="button" class="btn-secondary shrink-0" @click="store.setCustomer(null)">Change</button>
    </div>

    <div v-else class="relative">
      <label class="block">
        <span class="font-semibold">Customer <span class="font-normal text-muted">(optional)</span></span>
        <input
          v-model="q"
          type="search"
          class="field mt-1"
          inputmode="search"
          autocomplete="off"
          placeholder="Phone or name"
          @input="onInput"
        />
      </label>
      <ul v-if="results.length || canAdd" class="mt-2 divide-y divide-line rounded-control border border-line bg-surface">
        <li v-for="c in results" :key="c.id">
          <button type="button" class="flex min-h-12 w-full items-center justify-between gap-3 px-4 py-2 text-left hover:bg-canvas" @click="pick(c)">
            <span class="min-w-0">
              <span class="block truncate font-semibold">{{ c.name || fmtPhone(c.phone) }}</span>
              <span v-if="c.name" class="block text-sm text-muted">{{ fmtPhone(c.phone) }}</span>
            </span>
            <Badge v-if="toPaise(c.owes) > 0n" tone="golden">owes {{ formatRupees(c.owes) }}</Badge>
          </button>
        </li>
        <li v-if="canAdd" class="flex items-center gap-2 px-4 py-2">
          <input v-model="newName" class="field !py-2 min-w-0 flex-1" maxlength="100" placeholder="Name (optional)" aria-label="New customer name" @keydown.enter.prevent="addNew" />
          <button type="button" class="btn-secondary shrink-0" @click="addNew">Add {{ phoneShown }}</button>
        </li>
      </ul>
      <p v-else-if="searched" class="mt-2 text-sm text-muted">No customer matches. Type a 10-digit phone number to add one.</p>
    </div>
  </div>
</template>
