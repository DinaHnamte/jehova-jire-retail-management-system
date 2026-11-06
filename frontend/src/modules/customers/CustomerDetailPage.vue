<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { formatRupees, toPaise } from '@/shared/money'
import { openWhatsApp } from '@/shared/whatsapp'
import Badge from '@/shared/ui/Badge.vue'
import { getCustomer, updateCustomer, type CustomerDetail } from '@/modules/pos/api'
import { fmtPhone } from '@/modules/sales/format'
import { formatWhen } from '@/modules/stock/format'
import PaymentDialog from './PaymentDialog.vue'

const route = useRoute()
const session = useSessionStore()
const data = ref<CustomerDetail | null>(null)
const error = ref<string | null>(null)
const dialog = ref<InstanceType<typeof PaymentDialog> | null>(null)

const name = ref('')
const phone = ref('')
const note = ref('')
const editing = ref(false)
const saveError = ref<string | null>(null)
const saved = ref(false)
const busy = ref(false)

const id = () => String(route.params.id)

async function load() {
  error.value = null
  try {
    const d = await getCustomer(id())
    data.value = d
    if (!editing.value) fill(d)
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
function fill(d: CustomerDetail) {
  name.value = d.customer.name ?? ''
  phone.value = d.customer.phone
  note.value = d.customer.note ?? ''
}
onMounted(load)

const owes = computed(() => (data.value ? toPaise(data.value.owes) > 0n : false))
const canEdit = computed(() => session.can('orders.confirm'))
const canPay = computed(() => owes.value && session.can('payments.record'))

async function save() {
  saveError.value = null
  saved.value = false
  busy.value = true
  try {
    await updateCustomer(id(), { name: name.value.trim() || null, phone: phone.value.trim(), note: note.value.trim() || null })
    editing.value = false
    saved.value = true
    await load()
  } catch (e) {
    saveError.value = e instanceof ApiError ? e.message : friendlyMessage(e)
  } finally {
    busy.value = false
  }
}
function cancel() {
  editing.value = false
  saveError.value = null
  if (data.value) fill(data.value)
}
</script>

<template>
  <RouterLink to="/customers" class="text-muted hover:text-ink">← Customers</RouterLink>
  <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

  <template v-else-if="data">
    <div class="mt-2 flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <h1 class="truncate text-3xl font-extrabold">{{ data.customer.name || fmtPhone(data.customer.phone) }}</h1>
        <p v-if="data.customer.name" class="text-muted">{{ fmtPhone(data.customer.phone) }}</p>
        <p v-if="data.customer.note" class="mt-1 text-sm text-muted">{{ data.customer.note }}</p>
      </div>
    </div>

    <section class="card mt-5 p-5" aria-labelledby="owes-h">
      <h2 id="owes-h" class="text-sm text-muted">Owes</h2>
      <p class="figure mt-1 text-4xl" :class="owes ? 'text-golden-ink' : ''">{{ formatRupees(data.owes) }}</p>
      <div v-if="canPay || (owes && data.reminder_url)" class="mt-4 grid gap-3 sm:max-w-sm">
        <button v-if="canPay" type="button" class="btn-primary" @click="dialog?.open()">Record payment</button>
        <button v-if="owes && data.reminder_url" type="button" class="btn-secondary" @click="openWhatsApp(data.reminder_url)">Remind on WhatsApp</button>
      </div>
    </section>

    <section v-if="canEdit" class="card mt-5 p-5" aria-labelledby="edit-h">
      <div class="flex items-center justify-between gap-3">
        <h2 id="edit-h" class="text-xl font-bold">Details</h2>
        <button v-if="!editing" type="button" class="btn-secondary" @click="(editing = true, saved = false)">Edit</button>
      </div>
      <p v-if="saved" role="status" class="mt-3 rounded-control bg-sage-tint px-4 py-3 font-medium text-sage-ink">Saved.</p>
      <form v-if="editing" class="mt-3 grid gap-4" @submit.prevent="save">
        <label class="block"><span class="font-semibold">Name</span><input v-model="name" class="field mt-1" maxlength="120" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Phone</span><input v-model="phone" class="field mt-1" inputmode="tel" autocomplete="off" /></label>
        <label class="block"><span class="font-semibold">Note</span><input v-model="note" class="field mt-1" maxlength="200" /></label>
        <p v-if="saveError" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ saveError }}</p>
        <div class="flex justify-end gap-2">
          <button type="button" class="btn-secondary" @click="cancel">Cancel</button>
          <button type="submit" class="btn-primary" :disabled="busy">Save</button>
        </div>
      </form>
    </section>

    <section class="mt-6" aria-labelledby="sales-h">
      <h2 id="sales-h" class="text-xl font-bold">Sales</h2>
      <p v-if="!data.orders.length" class="mt-2 text-muted">No sales yet.</p>
      <ul v-else class="card mt-2 divide-y divide-line">
        <li v-for="o in data.orders" :key="o.id" class="relative grid grid-cols-[1fr_auto] gap-x-4 px-4 py-3">
          <div>
            <RouterLink :to="`/sales/${o.id}`" class="font-bold after:absolute after:inset-0 hover:text-primary-ink">{{ o.number }}</RouterLink>
            <p class="text-sm text-muted">{{ formatWhen(o.created_at) }}</p>
          </div>
          <div class="text-right">
            <p class="figure">{{ formatRupees(o.total) }}</p>
            <Badge v-if="o.status === 'cancelled'" tone="neutral">Cancelled</Badge>
            <Badge v-else-if="o.payment_status === 'returned'" tone="neutral">Returned</Badge>
            <Badge v-else-if="toPaise(o.owed) > 0n" tone="golden">Owed {{ formatRupees(o.owed) }}</Badge>
          </div>
        </li>
      </ul>
    </section>

    <section class="mt-6" aria-labelledby="pays-h">
      <h2 id="pays-h" class="text-xl font-bold">Payments</h2>
      <p v-if="!data.payments.length" class="mt-2 text-muted">No payments yet.</p>
      <ul v-else class="card mt-2 divide-y divide-line">
        <li v-for="p in data.payments" :key="p.id" class="flex justify-between px-4 py-3">
          <span class="text-sm text-muted">{{ formatWhen(p.occurred_at) }} · {{ p.method.toUpperCase() }}</span>
          <span class="figure">{{ formatRupees(p.amount) }}</span>
        </li>
      </ul>
    </section>

    <PaymentDialog ref="dialog" :customer-id="data.customer.id" :owes="data.owes" @saved="load" />
  </template>
</template>
