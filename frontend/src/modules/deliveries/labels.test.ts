import { describe, expect, it } from 'vitest'
import { reasonLabel, reasons, statusLabel } from './labels'

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
