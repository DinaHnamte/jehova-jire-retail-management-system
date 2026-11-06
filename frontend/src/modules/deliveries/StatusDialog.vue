<script setup lang="ts">
import { computed, ref } from 'vue'
import { setStatus, type StatusBody } from './api'
import { conflictStatus, friendlyMessage, isStatusConflict } from './errors'
import { reasons, type FailReason } from './labels'

const props = defineProps<{ deliveryId: string; to: 'picked_up' | 'delivered' | 'failed' }>()
const emit = defineEmits<{ done: []; stale: [message: string | null] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const receivedBy = ref('')
const reason = ref<FailReason | ''>('')
const note = ref('')
const error = ref<string | null>(null)
const busy = ref(false)

const title = computed(() =>
  props.to === 'picked_up' ? 'Mark as picked up?' : props.to === 'delivered' ? 'Mark as delivered' : "Mark as couldn't deliver",
)

function open() {
  error.value = null
  receivedBy.value = ''
  reason.value = ''
  note.value = ''
  dialog.value?.showModal()
}
defineExpose({ open })

function onCancel(e: Event) {
  if (busy.value) e.preventDefault()
}

async function submit() {
  if (busy.value) return
  error.value = null
  const body: StatusBody = { to: props.to }
  if (props.to === 'delivered') {
    if (!receivedBy.value.trim()) {
      error.value = 'Enter who received it.'
      return
    }
    body.received_by = receivedBy.value.trim()
  }
  if (props.to === 'failed') {
    if (!reason.value) {
      error.value = 'Choose why it could not be delivered.'
      return
    }
    body.reason = reason.value
    if (reason.value === 'other') {
      if (!note.value.trim()) {
        error.value = 'Add a note saying what happened.'
        return
      }
    }
    if (note.value.trim()) body.note = note.value.trim()
  }
  busy.value = true
  try {
    await setStatus(props.deliveryId, body)
    dialog.value?.close()
    emit('done')
  } catch (e) {
    if (isStatusConflict(e)) {
      dialog.value?.close()
      // Already in the status asked for (a double tap): that is success.
      emit('stale', conflictStatus(e) === props.to ? null : e.message)
    } else error.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="status-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    @cancel="onCancel"
  >
    <form class="p-5 sm:p-6" @submit.prevent="submit">
      <h2 id="status-title" class="text-xl font-bold">{{ title }}</h2>
      <p v-if="to === 'picked_up'" class="mt-2 text-muted">The fee, pay and address can't be changed after this.</p>
      <label v-if="to === 'delivered'" class="mt-4 block">
        <span class="font-semibold">Who received it?</span>
        <input v-model="receivedBy" class="field mt-1" maxlength="80" autocomplete="off" :disabled="busy" />
      </label>
      <template v-if="to === 'failed'">
        <label class="mt-4 block">
          <span class="font-semibold">Why?</span>
          <select v-model="reason" class="field mt-1" :disabled="busy">
            <option value="" disabled>Choose a reason</option>
            <option v-for="r in reasons" :key="r.code" :value="r.code">{{ r.label }}</option>
          </select>
        </label>
        <label class="mt-4 block">
          <span class="font-semibold">Note <span v-if="reason !== 'other'" class="font-normal text-muted">(optional)</span></span>
          <input v-model="note" class="field mt-1" maxlength="500" autocomplete="off" :disabled="busy" />
        </label>
      </template>
      <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="dialog?.close()">Close</button>
        <button type="submit" class="btn-primary" :disabled="busy">Confirm</button>
      </div>
    </form>
  </dialog>
</template>
