import { describe, expect, it } from 'vitest'
import { collectLine, deliveryBody, draftErrors, draftFromLastAddress, emptyDraft, totalWithFee } from './delivery'

const full = () => ({
  ...emptyDraft(),
  recipientName: '  Lalmawia ',
  recipientPhone: ' 98620 00000 ',
  addressLine: ' Zarkawt 12 ',
  locality: ' Zarkawt ',
  fee: '50',
})

describe('deliveryBody', () => {
  it('trims, nulls blanks and omits a blank driver pay', () => {
    const b = deliveryBody(full())!
    expect(b).toMatchObject({
      recipient_name: 'Lalmawia',
      recipient_phone: '98620 00000',
      address_line: 'Zarkawt 12',
      locality: 'Zarkawt',
      landmark: null,
      instructions: null,
      fee: '50.00',
      collector: 'driver',
      pay_method: 'retain_from_collection',
    })
    expect('driver_pay' in b).toBe(false)
  })
  it('sends a typed driver pay', () => {
    expect(deliveryBody({ ...full(), driverPay: '30' })!.driver_pay).toBe('30.00')
  })
  it('forces paid_by_shop when the shop collects', () => {
    expect(deliveryBody({ ...full(), collector: 'shop' })!.pay_method).toBe('paid_by_shop')
  })
})

describe('draftErrors', () => {
  it('requires name, phone, address and locality', () => {
    expect(Object.keys(draftErrors(emptyDraft())).sort()).toEqual([
      'delivery.address_line',
      'delivery.fee',
      'delivery.locality',
      'delivery.recipient_name',
      'delivery.recipient_phone',
    ])
    expect(draftErrors(full())).toEqual({})
  })
  it('rejects a fee that is not an amount', () => {
    expect(draftErrors({ ...full(), fee: 'abc' })['delivery.fee']).toBeTruthy()
  })
})

describe('totalWithFee', () => {
  it('adds in paise', () => {
    expect(totalWithFee('200.00', '50')).toBe('250.00')
    expect(totalWithFee('0.10', '0.20')).toBe('0.30')
  })
  it('treats a blank or bad fee as zero', () => {
    expect(totalWithFee('200.00', '')).toBe('200.00')
    expect(totalWithFee('200.00', 'x')).toBe('200.00')
  })
})

describe('draftFromLastAddress', () => {
  const addr = { recipient_name: 'Old', recipient_phone: '9000000000', address_line: 'Old street', locality: 'Old loc', landmark: 'Tree', instructions: null }
  it('fills empty fields only', () => {
    const d = draftFromLastAddress(addr, { name: 'Cust', phone: '9111111111' }, { ...emptyDraft(), addressLine: 'Typed' })
    expect(d.addressLine).toBe('Typed')
    expect(d.recipientName).toBe('Old')
    expect(d.landmark).toBe('Tree')
  })
  it('falls back to the customer with no previous delivery', () => {
    const d = draftFromLastAddress(null, { name: 'Cust', phone: '9111111111' }, emptyDraft())
    expect(d.recipientName).toBe('Cust')
    expect(d.recipientPhone).toBe('9111111111')
  })
})

describe('collectLine', () => {
  it('words who collects', () => {
    expect(collectLine('driver', 25000n)).toBe('Driver collects ₹250.00')
    expect(collectLine('shop', 25000n)).toBe('You collect ₹250.00')
    expect(collectLine('driver', 0n)).toBe('Paid in full')
  })
})
