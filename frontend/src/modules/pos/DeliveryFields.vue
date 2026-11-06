<script setup lang="ts">
import type { DeliveryDraft } from './delivery'

const draft = defineModel<DeliveryDraft>({ required: true })
defineProps<{ errors: Record<string, string>; disabled?: boolean }>()

function set<K extends keyof DeliveryDraft>(key: K, value: DeliveryDraft[K]) {
  draft.value = { ...draft.value, [key]: value }
}
const onInput = (key: keyof DeliveryDraft) => (e: Event) => set(key, (e.target as HTMLInputElement).value as never)
</script>

<template>
  <div class="mt-4 grid gap-3">
    <label class="block">
      <span class="font-semibold">Recipient name</span>
      <input :disabled="disabled" name="delivery.recipient_name" class="field mt-1" maxlength="100" autocomplete="off" :value="draft.recipientName" :aria-invalid="!!errors['delivery.recipient_name']" @input="onInput('recipientName')" />
      <span v-if="errors['delivery.recipient_name']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.recipient_name'] }}</span>
    </label>
    <label class="block">
      <span class="font-semibold">Recipient phone</span>
      <input :disabled="disabled" name="delivery.recipient_phone" class="field mt-1" type="tel" inputmode="tel" autocomplete="off" :value="draft.recipientPhone" :aria-invalid="!!errors['delivery.recipient_phone']" @input="onInput('recipientPhone')" />
      <span v-if="errors['delivery.recipient_phone']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.recipient_phone'] }}</span>
    </label>
    <label class="block">
      <span class="font-semibold">Address</span>
      <input :disabled="disabled" name="delivery.address_line" class="field mt-1" maxlength="300" autocomplete="off" :value="draft.addressLine" :aria-invalid="!!errors['delivery.address_line']" @input="onInput('addressLine')" />
      <span v-if="errors['delivery.address_line']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.address_line'] }}</span>
    </label>
    <div class="grid grid-cols-2 gap-3">
      <label class="block">
        <span class="font-semibold">Locality</span>
        <input :disabled="disabled" name="delivery.locality" class="field mt-1" maxlength="100" autocomplete="off" :value="draft.locality" :aria-invalid="!!errors['delivery.locality']" @input="onInput('locality')" />
        <span v-if="errors['delivery.locality']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.locality'] }}</span>
      </label>
      <label class="block">
        <span class="font-semibold">Landmark <span class="font-normal text-muted">(optional)</span></span>
        <input :disabled="disabled" name="delivery.landmark" class="field mt-1" maxlength="200" autocomplete="off" :value="draft.landmark" :aria-invalid="!!errors['delivery.landmark']" @input="onInput('landmark')" />
        <span v-if="errors['delivery.landmark']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.landmark'] }}</span>
      </label>
    </div>
    <label class="block">
      <span class="font-semibold">Instructions <span class="font-normal text-muted">(optional)</span></span>
      <input :disabled="disabled" name="delivery.instructions" class="field mt-1" maxlength="500" autocomplete="off" :value="draft.instructions" :aria-invalid="!!errors['delivery.instructions']" @input="onInput('instructions')" />
      <span v-if="errors['delivery.instructions']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.instructions'] }}</span>
    </label>
    <div class="grid grid-cols-2 gap-3">
      <label class="block">
        <span class="font-semibold">Delivery fee ₹</span>
        <input :disabled="disabled" name="delivery.fee" class="field mt-1" inputmode="decimal" autocomplete="off" placeholder="0" :value="draft.fee" :aria-invalid="!!errors['delivery.fee']" @input="onInput('fee')" />
        <span v-if="errors['delivery.fee']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.fee'] }}</span>
      </label>
      <label class="block">
        <span class="font-semibold">Driver pay ₹ <span class="font-normal text-muted">(optional)</span></span>
        <input :disabled="disabled" name="delivery.driver_pay" class="field mt-1" inputmode="decimal" autocomplete="off" placeholder="Default" :value="draft.driverPay" :aria-invalid="!!errors['delivery.driver_pay']" @input="onInput('driverPay')" />
        <span v-if="errors['delivery.driver_pay']" class="mt-1 block text-sm font-medium text-danger">{{ errors['delivery.driver_pay'] }}</span>
      </label>
    </div>

    <fieldset>
      <legend class="font-semibold">Who collects</legend>
      <div class="mt-1 grid grid-cols-2 gap-2" role="radiogroup">
        <button type="button" role="radio" :disabled="disabled" :aria-checked="draft.collector === 'driver'" :class="draft.collector === 'driver' ? 'btn-primary' : 'btn-secondary'" @click="set('collector', 'driver')">Driver</button>
        <button type="button" role="radio" :disabled="disabled" :aria-checked="draft.collector === 'shop'" :class="draft.collector === 'shop' ? 'btn-primary' : 'btn-secondary'" @click="set('collector', 'shop')">Me</button>
      </div>
      <p v-if="errors['delivery.collector']" class="mt-1 text-sm font-medium text-danger">{{ errors['delivery.collector'] }}</p>
    </fieldset>
    <fieldset v-if="draft.collector === 'driver'">
      <legend class="font-semibold">Driver's pay</legend>
      <div class="mt-1 grid grid-cols-2 gap-2" role="radiogroup">
        <button type="button" role="radio" :disabled="disabled" :aria-checked="draft.payMethod === 'retain_from_collection'" :class="draft.payMethod === 'retain_from_collection' ? 'btn-primary' : 'btn-secondary'" @click="set('payMethod', 'retain_from_collection')">Keeps it from the cash</button>
        <button type="button" role="radio" :disabled="disabled" :aria-checked="draft.payMethod === 'paid_by_shop'" :class="draft.payMethod === 'paid_by_shop' ? 'btn-primary' : 'btn-secondary'" @click="set('payMethod', 'paid_by_shop')">I pay them</button>
      </div>
      <p v-if="errors['delivery.pay_method']" class="mt-1 text-sm font-medium text-danger">{{ errors['delivery.pay_method'] }}</p>
    </fieldset>
  </div>
</template>
