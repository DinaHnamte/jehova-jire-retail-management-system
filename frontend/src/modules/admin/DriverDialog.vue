<script setup lang="ts">
import { ref } from 'vue'
import { friendlyMessage } from '@/shared/api/errors'
import { parseMoneyInput } from '@/shared/money'
import {
  buildDriverPatch,
  createDriver,
  errorField,
  MIN_PASSWORD_LEN,
  resetDriverPassword,
  updateDriver,
  type Driver,
} from './drivers-api'

const emit = defineEmits<{ saved: [] }>()

type Mode = 'add' | 'edit' | 'password'
const dialog = ref<HTMLDialogElement | null>(null)
const mode = ref<Mode>('add')
const target = ref<Driver | null>(null)
const name = ref('')
const phone = ref('')
const username = ref('')
const password = ref('')
const pay = ref('')
const note = ref('')
const errors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const busy = ref(false)

function open(m: Mode, driver: Driver | null = null) {
  mode.value = m
  target.value = driver
  name.value = driver?.display_name ?? ''
  phone.value = driver?.phone ?? ''
  username.value = ''
  password.value = ''
  pay.value = driver?.default_pay ?? ''
  note.value = driver?.note ?? ''
  errors.value = {}
  formError.value = null
  dialog.value?.showModal()
}
defineExpose({ open })

const titles: Record<Mode, string> = { add: 'Add driver', edit: 'Edit driver', password: 'Reset password' }

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

function fail(e: unknown) {
  const field = errorField(e)
  const message = friendlyMessage(e)
  if (field) errors.value = { [field]: message }
  else if (/^password/i.test(message)) errors.value = { password: message }
  else if (mode.value === 'add' && /username/i.test(message)) errors.value = { username: message }
  else formError.value = message
}

async function submit() {
  if (busy.value) return
  errors.value = {}
  formError.value = null
  const local: Record<string, string> = {}
  let payStr = ''
  if (mode.value !== 'password') {
    if (!name.value.trim()) local.display_name = 'Enter the driver’s name.'
    if (!phone.value.trim()) local.phone = 'Enter a phone number.'
    const p = parseMoneyInput(pay.value)
    if (p === null) local.default_pay = 'Enter an amount like 50 or 50.00.'
    else payStr = p
  }
  if (mode.value === 'add' && !username.value.trim()) local.username = 'Enter a username for signing in.'
  if (mode.value !== 'edit' && password.value.length < MIN_PASSWORD_LEN)
    local.password = `Use at least ${MIN_PASSWORD_LEN} characters.`
  if (Object.keys(local).length) {
    errors.value = local
    return
  }
  busy.value = true
  try {
    if (mode.value === 'add') {
      await createDriver({
        display_name: name.value.trim(),
        username: username.value.trim(),
        password: password.value,
        phone: phone.value.trim(),
        default_pay: payStr,
        ...(note.value.trim() ? { note: note.value.trim() } : {}),
      })
    } else if (mode.value === 'edit' && target.value) {
      const patch = buildDriverPatch(target.value, {
        display_name: name.value,
        phone: phone.value,
        default_pay: payStr,
        note: note.value,
      })
      if (Object.keys(patch).length) await updateDriver(target.value.id, patch)
    } else if (target.value) {
      await resetDriverPassword(target.value.id, password.value)
    }
    dialog.value?.close()
    emit('saved')
  } catch (e) {
    fail(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="driver-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" novalidate @submit.prevent="submit">
      <h2 id="driver-title" class="text-xl font-bold">{{ titles[mode] }}</h2>
      <p v-if="mode === 'password'" class="mt-2 text-muted">
        Sets a new password for {{ target?.display_name }}. They are signed out everywhere.
      </p>
      <template v-if="mode !== 'password'">
        <label class="mt-4 block">
          <span class="font-semibold">Name</span>
          <input v-model="name" class="field mt-1" maxlength="80" autocomplete="off" :disabled="busy" />
          <span v-if="errors.display_name" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.display_name }}</span>
        </label>
        <label class="mt-4 block">
          <span class="font-semibold">Phone</span>
          <input v-model="phone" class="field mt-1" type="tel" inputmode="tel" autocomplete="off" :disabled="busy" />
          <span v-if="errors.phone" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.phone }}</span>
        </label>
      </template>
      <label v-if="mode === 'add'" class="mt-4 block">
        <span class="font-semibold">Username</span>
        <input v-model="username" class="field mt-1" autocomplete="off" autocapitalize="none" :disabled="busy" />
        <span v-if="errors.username" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.username }}</span>
      </label>
      <label v-if="mode !== 'edit'" class="mt-4 block">
        <span class="font-semibold">{{ mode === 'add' ? 'Password' : 'New password' }}</span>
        <input v-model="password" class="field mt-1" type="password" autocomplete="new-password" :disabled="busy" />
        <span class="mt-1 block text-sm text-muted">At least {{ MIN_PASSWORD_LEN }} characters.</span>
        <span v-if="errors.password" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.password }}</span>
      </label>
      <template v-if="mode !== 'password'">
        <label class="mt-4 block">
          <span class="font-semibold">Default pay per trip (₹)</span>
          <input v-model="pay" class="field mt-1" inputmode="decimal" autocomplete="off" :disabled="busy" />
          <span v-if="errors.default_pay" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.default_pay }}</span>
        </label>
        <label class="mt-4 block">
          <span class="font-semibold">Note <span class="font-normal text-muted">(optional)</span></span>
          <input v-model="note" class="field mt-1" maxlength="500" autocomplete="off" :disabled="busy" />
          <span v-if="errors.note" role="alert" class="mt-1 block text-sm font-medium text-danger">{{ errors.note }}</span>
        </label>
      </template>
      <p v-if="formError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Close</button>
        <button type="submit" class="btn-primary" :disabled="busy">Save</button>
      </div>
    </form>
  </dialog>
</template>
