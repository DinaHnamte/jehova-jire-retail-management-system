<script setup lang="ts">
import { nextTick, ref } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { adjustStock, type AdjustType, type ProductStock } from './api'
import { parseQuantity } from './totals'

const props = defineProps<{ productId: string; onHand: number }>()
const emit = defineEmits<{ saved: [stock: ProductStock] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const type = ref<AdjustType>('adjustment_out')
const quantity = ref('')
const reason = ref('')
const errors = ref<Record<string, string>>({})
const formError = ref<string | null>(null)
const busy = ref(false)

const types: { value: AdjustType; label: string; hint: string }[] = [
  { value: 'adjustment_in', label: 'Add', hint: 'Found more than the app shows' },
  { value: 'adjustment_out', label: 'Remove', hint: 'Fewer than the app shows' },
  { value: 'damage', label: 'Damaged', hint: 'Broken or unsellable' },
]

async function open() {
  type.value = 'adjustment_out'
  quantity.value = ''
  reason.value = ''
  errors.value = {}
  formError.value = null
  dialog.value?.showModal()
  await nextTick()
  dialog.value?.querySelector<HTMLInputElement>('input[name=quantity]')?.focus()
}
defineExpose({ open })

async function save() {
  errors.value = {}
  formError.value = null
  const q = parseQuantity(quantity.value)
  if (q === null) errors.value.quantity = 'Enter a whole number from 1 to 1,000,000.'
  if (reason.value.trim().length < 3) errors.value.reason = 'Say why in 3 to 200 characters.'
  if (Object.keys(errors.value).length || q === null) return
  busy.value = true
  try {
    const stock = await adjustStock(props.productId, { type: type.value, quantity: q, reason: reason.value })
    dialog.value?.close()
    emit('saved', stock)
  } catch (e) {
    const field = e instanceof ApiError ? (e.details as { field?: string })?.field : undefined
    if (e instanceof ApiError && e.code === 'INSUFFICIENT_STOCK') errors.value.quantity = e.message
    else if (field) errors.value[field] = friendlyMessage(e)
    else formError.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="adjust-title"
    class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
  >
    <form class="p-5 sm:p-6" @submit.prevent="save">
      <h2 id="adjust-title" class="text-xl font-bold">Adjust stock</h2>
      <p class="mt-1 text-muted">On hand now: <span class="font-bold text-ink">{{ onHand }}</span></p>

      <fieldset class="mt-4">
        <legend class="font-semibold">What happened?</legend>
        <div class="mt-2 grid gap-2">
          <label
            v-for="t in types"
            :key="t.value"
            class="flex cursor-pointer items-center gap-3 rounded-control border px-3.5 py-2.5"
            :class="type === t.value ? 'border-primary bg-primary-tint' : 'border-line'"
          >
            <input v-model="type" type="radio" name="type" :value="t.value" class="accent-primary" />
            <span><span class="font-semibold">{{ t.label }}</span> <span class="text-sm text-muted">{{ t.hint }}</span></span>
          </label>
        </div>
      </fieldset>

      <label class="mt-4 block">
        <span class="font-semibold">How many</span>
        <input
          v-model="quantity"
          name="quantity"
          class="field mt-1"
          inputmode="numeric"
          autocomplete="off"
          :aria-invalid="!!errors.quantity"
          aria-describedby="adjust-qty-err"
        />
        <span v-if="errors.quantity" id="adjust-qty-err" class="mt-1 block text-sm font-medium text-danger">{{ errors.quantity }}</span>
      </label>

      <label class="mt-4 block">
        <span class="font-semibold">Reason</span>
        <input
          v-model="reason"
          class="field mt-1"
          maxlength="200"
          placeholder="e.g. Cracked when unpacking"
          :aria-invalid="!!errors.reason"
          aria-describedby="adjust-reason-err"
        />
        <span v-if="errors.reason" id="adjust-reason-err" class="mt-1 block text-sm font-medium text-danger">{{ errors.reason }}</span>
      </label>

      <p v-if="formError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>

      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="btn-secondary" @click="dialog?.close()">Cancel</button>
        <button type="submit" class="btn-primary" :disabled="busy">Save adjustment</button>
      </div>
    </form>
  </dialog>
</template>
