<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ApiError } from '@/shared/api/errors'
import { isActiveJob, myJob, setMyStatus, type JobDetail, type MyJob, type StatusBody } from '@/modules/deliveries/api'
import { friendlyMessage } from '@/modules/deliveries/errors'
import { reasonLabel, reasons, statusLabel, type FailReason } from '@/modules/deliveries/labels'
import { formatRupees } from '@/shared/money'
import { canFail, primaryAction, statusConflictIsDone } from './actions'

const route = useRoute()
const router = useRouter()
const id = String(route.params.id)

const job = ref<MyJob | null>(null)
const error = ref<string | null>(null)
const gone = ref(false)
const busy = ref(false)
const mode = ref<'delivered' | 'failed' | null>(null)
const receivedBy = ref('')
const reason = ref<FailReason | ''>('')
const note = ref('')
const formError = ref<string | null>(null)
let backTimer: ReturnType<typeof setTimeout> | undefined

const detail = computed<JobDetail | null>(() => (job.value && isActiveJob(job.value) ? job.value : null))
const collect = computed(() => (detail.value && Number(detail.value.amount_to_collect) > 0 ? detail.value.amount_to_collect : null))
const next = computed(() => (detail.value ? primaryAction(detail.value.status) : null))
const mayFail = computed(() => (detail.value ? canFail(detail.value.status) : false))
const tel = computed(() => (detail.value ? `tel:${detail.value.recipient_phone.replace(/[^\d+]/g, '')}` : ''))

const backToList = () => router.replace('/driver')

async function load() {
  try {
    job.value = await myJob(id)
    error.value = null
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) {
      gone.value = true
      backTimer = setTimeout(backToList, 3000)
    } else error.value = friendlyMessage(e)
  }
}
onMounted(load)
onBeforeUnmount(() => clearTimeout(backTimer))

function openForm(m: 'delivered' | 'failed') {
  mode.value = m
  formError.value = null
}

async function send(body: StatusBody) {
  if (busy.value) return
  busy.value = true
  error.value = null
  try {
    await setMyStatus(id, body)
    // Delivered and failed jobs are history now; picked up stays on this page.
    if (body.to === 'picked_up') {
      mode.value = null
      await load()
    } else await backToList()
  } catch (e) {
    if (e instanceof ApiError && statusConflictIsDone(e, body.to)) {
      if (body.to === 'picked_up') await load()
      else await backToList()
    } else if (e instanceof ApiError && e.status === 404) {
      mode.value = null
      gone.value = true
      backTimer = setTimeout(backToList, 3000)
    } else {
      const msg = friendlyMessage(e)
      if (mode.value) formError.value = msg
      else error.value = msg
      if (e instanceof ApiError && e.status === 409) await load()
    }
  } finally {
    busy.value = false
  }
}

function confirmDelivered() {
  const who = receivedBy.value.trim()
  if (!who) return void (formError.value = 'Enter who received it.')
  void send({ to: 'delivered', received_by: who })
}

function confirmFailed() {
  if (!reason.value) return void (formError.value = 'Choose why it could not be delivered.')
  const n = note.value.trim()
  if (reason.value === 'other' && !n) return void (formError.value = 'Add a note saying what happened.')
  void send({ to: 'failed', reason: reason.value, ...(n ? { note: n } : {}) })
}
</script>

<template>
  <div>
    <RouterLink to="/driver" class="inline-flex min-h-12 items-center font-semibold text-primary-ink underline underline-offset-4">All deliveries</RouterLink>

    <div v-if="gone" role="alert" class="mt-2 rounded-card border border-line bg-surface p-5">
      <p class="text-lg font-bold">This job is no longer assigned to you.</p>
      <button type="button" class="btn-primary mt-4 w-full" @click="backToList">Back to my deliveries</button>
    </div>

    <p v-else-if="error && !job" role="alert" class="mt-2 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
    <p v-else-if="!job" class="mt-2 text-muted">Loading…</p>

    <div v-else-if="!detail" class="mt-2 rounded-card border border-line bg-surface p-5">
      <p class="display text-2xl font-extrabold">{{ job.locality }}</p>
      <p class="mt-1 text-muted">{{ job.order_number }} · {{ statusLabel(job.status) }}</p>
      <button type="button" class="btn-primary mt-4 w-full" @click="backToList">Back to my deliveries</button>
    </div>

    <div v-else class="mt-2 space-y-4">
      <div>
        <p class="display text-3xl leading-tight font-extrabold">{{ detail.locality }}</p>
        <p class="mt-1 text-muted">{{ detail.order_number }} · {{ statusLabel(detail.status) }}</p>
      </div>

      <p v-if="error" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

      <div class="grid grid-cols-2 gap-3">
        <a :href="tel" class="btn-primary min-h-14">Call</a>
        <a :href="detail.maps_url" target="_blank" rel="noopener" class="btn-secondary min-h-14 text-[1.1875rem] font-bold">Map</a>
      </div>

      <section class="card space-y-3 p-4">
        <div>
          <p class="text-sm font-semibold text-muted">{{ detail.recipient_name }} · {{ detail.recipient_phone }}</p>
          <p class="mt-1 text-lg">{{ detail.address_line }}, {{ detail.locality }}</p>
        </div>
        <p v-if="detail.landmark"><span class="font-semibold">Landmark:</span> {{ detail.landmark }}</p>
        <p v-if="detail.instructions"><span class="font-semibold">Note:</span> {{ detail.instructions }}</p>
      </section>

      <section class="card p-4">
        <h2 class="font-bold">Items</h2>
        <ul class="mt-2 space-y-1">
          <li v-for="(it, i) in detail.items" :key="i" class="flex justify-between gap-3">
            <span>{{ it.name }}</span><span class="font-semibold">× {{ it.quantity }}</span>
          </li>
        </ul>
      </section>

      <p class="figure rounded-card bg-golden-tint p-4 text-2xl text-golden-ink">
        {{ collect ? `Collect ${formatRupees(collect)}` : 'Already paid' }}
      </p>

      <template v-if="mode === 'delivered'">
        <form class="card space-y-3 p-4" @submit.prevent="confirmDelivered">
          <label class="block">
            <span class="font-semibold">Who received it?</span>
            <input v-model="receivedBy" class="field mt-1" maxlength="80" autocomplete="off" :disabled="busy" />
          </label>
          <p v-if="formError" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>
          <button type="submit" class="btn-primary w-full" :disabled="busy">Confirm delivered</button>
          <button type="button" class="btn-secondary min-h-12 w-full" :disabled="busy" @click="mode = null">Back</button>
        </form>
      </template>

      <template v-else-if="mode === 'failed'">
        <form class="card space-y-3 p-4" @submit.prevent="confirmFailed">
          <fieldset :disabled="busy">
            <legend class="font-semibold">Why couldn't you deliver?</legend>
            <label
              v-for="r in reasons"
              :key="r.code"
              class="mt-2 flex min-h-12 items-center gap-3 rounded-control border border-line px-4 py-2"
            >
              <input v-model="reason" type="radio" name="reason" :value="r.code" class="size-5 accent-primary" />
              <span>{{ reasonLabel(r.code) }}</span>
            </label>
          </fieldset>
          <label class="block">
            <span class="font-semibold">Note <span v-if="reason !== 'other'" class="font-normal text-muted">(optional)</span></span>
            <input v-model="note" class="field mt-1" maxlength="500" autocomplete="off" :disabled="busy" />
          </label>
          <p v-if="formError" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>
          <button type="submit" class="btn-primary w-full" :disabled="busy">Confirm couldn't deliver</button>
          <button type="button" class="btn-secondary min-h-12 w-full" :disabled="busy" @click="mode = null">Back</button>
        </form>
      </template>

      <template v-else>
        <button
          v-if="next === 'picked_up'"
          type="button"
          class="btn-primary min-h-14 w-full"
          :disabled="busy"
          @click="send({ to: 'picked_up' })"
        >
          Picked up
        </button>
        <button v-else-if="next === 'delivered'" type="button" class="btn-primary min-h-14 w-full" :disabled="busy" @click="openForm('delivered')">
          Delivered
        </button>
        <button v-if="mayFail" type="button" class="btn-secondary min-h-12 w-full" :disabled="busy" @click="openForm('failed')">
          Couldn't deliver
        </button>
      </template>
    </div>
  </div>
</template>
