import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import DeliveryFields from './DeliveryFields.vue'
import { emptyDraft, type DeliveryDraft } from './delivery'

describe('DeliveryFields', () => {
  it('sends what is typed in every box up to the draft', async () => {
    let draft: DeliveryDraft = emptyDraft()
    const w = mount(DeliveryFields, {
      props: { modelValue: draft, errors: {}, 'onUpdate:modelValue': (d: DeliveryDraft) => { draft = d; w.setProps({ modelValue: d }) } },
    })
    const typed: [string, keyof DeliveryDraft, string][] = [
      ['delivery.recipient_name', 'recipientName', 'Mawia'],
      ['delivery.recipient_phone', 'recipientPhone', '9862000000'],
      ['delivery.address_line', 'addressLine', 'Zarkawt 12'],
      ['delivery.locality', 'locality', 'Zarkawt'],
      ['delivery.landmark', 'landmark', 'Near church'],
      ['delivery.instructions', 'instructions', 'Call first'],
      ['delivery.fee', 'fee', '50'],
      ['delivery.driver_pay', 'driverPay', '30'],
    ]
    for (const [name, , value] of typed) await w.find(`input[name="${name}"]`).setValue(value)
    for (const [, key, value] of typed) expect(draft[key]).toBe(value)
  })
})
