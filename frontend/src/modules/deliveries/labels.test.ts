import { describe, expect, it } from 'vitest'
import { actionsFor, ageLabel, moneyLabel, reasonLabel, reasons, saleCancelNote, statusLabel } from './labels'

describe('statusLabel', () => {
  it('labels every status', () => {
    expect(statusLabel('to_send')).toBe('To send')
    expect(statusLabel('assigned')).toBe('Assigned')
    expect(statusLabel('picked_up')).toBe('Out for delivery')
    expect(statusLabel('delivered')).toBe('Delivered')
    expect(statusLabel('failed')).toBe("Couldn't deliver")
    expect(statusLabel('returned')).toBe('Returned to shop')
    expect(statusLabel('cancelled')).toBe('Cancelled')
  })
  it('falls back to the code', () => {
    expect(statusLabel('mystery')).toBe('mystery')
  })
})

describe('reasonLabel', () => {
  it('labels every reason, in order', () => {
    expect(reasons.map((r) => r.code)).toEqual([
      'customer_unavailable',
      'address_not_found',
      'customer_refused',
      'unreachable_by_phone',
      'other',
    ])
    expect(reasonLabel('customer_unavailable')).toBe('Customer not there')
    expect(reasonLabel('address_not_found')).toBe("Couldn't find the address")
    expect(reasonLabel('customer_refused')).toBe('Customer refused')
    expect(reasonLabel('unreachable_by_phone')).toBe("Customer didn't answer the phone")
    expect(reasonLabel('other')).toBe('Other')
  })
  it('falls back to the code', () => {
    expect(reasonLabel('zzz')).toBe('zzz')
  })
})

describe('actionsFor', () => {
  it('lists what the owner can do per status', () => {
    expect(actionsFor('to_send', false)).toEqual(['assign', 'edit', 'cancel'])
    expect(actionsFor('assigned', true)).toEqual(['reassign', 'edit', 'cancel', 'picked_up'])
    expect(actionsFor('picked_up', true)).toEqual(['delivered', 'failed'])
    expect(actionsFor('failed', true)).toEqual(['retry', 'check_in'])
  })
  it('offers nothing once closed', () => {
    for (const s of ['delivered', 'returned', 'cancelled'] as const) expect(actionsFor(s, true)).toEqual([])
  })
})

describe('ageLabel', () => {
  const now = new Date('2026-11-07T12:00:00Z')
  it('reads minutes, hours and days', () => {
    expect(ageLabel('2026-11-07T11:50:00Z', now)).toBe('10 min')
    expect(ageLabel('2026-11-07T10:00:00Z', now)).toBe('2 h')
    expect(ageLabel('2026-11-04T12:00:00Z', now)).toBe('3 d')
    expect(ageLabel('2026-11-07T12:00:30Z', now)).toBe('now')
  })
})

describe('saleCancelNote', () => {
  it('explains why a delivery sale cannot be cancelled', () => {
    expect(saleCancelNote('picked_up')).toBe('This delivery is out with the driver. Mark it delivered or couldn\'t deliver first.')
    expect(saleCancelNote('failed')).toBe("This delivery couldn't be delivered. Check the goods back in to cancel it.")
    expect(saleCancelNote('delivered')).toBe('This delivery is done. Return items instead.')
    expect(saleCancelNote('returned')).toBeNull()
    expect(saleCancelNote('assigned')).toBeNull()
  })
})

describe('moneyLabel', () => {
  it('says the driver collects what is owed', () => {
    expect(moneyLabel({ collector: 'driver', owed: '150.00' })).toBe('Collect ₹150.00')
  })
  it('says the shop collects when money is still owed', () => {
    expect(moneyLabel({ collector: 'shop', owed: '150.00' })).toBe('Shop collects ₹150.00')
  })
  it('says paid only when nothing is owed', () => {
    expect(moneyLabel({ collector: 'driver', owed: '0.00' })).toBe('Paid')
    expect(moneyLabel({ collector: 'shop', owed: '0.00' })).toBe('Paid')
  })
})
