import { describe, expect, it } from 'vitest'
import { formatCost, formatDay, formatWhen, movementLabel, signed } from './format'

describe('stock formatting', () => {
  it('labels movement types in plain words', () => {
    expect(movementLabel('opening_balance')).toBe('Opening count')
    expect(movementLabel('purchase_receipt')).toBe('Received')
    expect(movementLabel('adjustment_in')).toBe('Added')
    expect(movementLabel('adjustment_out')).toBe('Removed')
    expect(movementLabel('damage')).toBe('Damaged')
    expect(movementLabel('sale')).toBe('Sold')
    expect(movementLabel('sale_cancel')).toBe('Sale cancelled')
    expect(movementLabel('sale_return')).toBe('Returned')
  })
  it('signs quantities', () => {
    expect(signed(5)).toBe('+5')
    expect(signed(-2)).toBe('−2')
    expect(signed(0)).toBe('0')
  })
  it('rounds average cost to paise for display', () => {
    expect(formatCost('110.0000')).toBe('₹110.00')
    expect(formatCost('10.3333')).toBe('₹10.33')
    expect(formatCost('10.3350')).toBe('₹10.34')
    expect(formatCost(null)).toBe('—')
  })
  it('shows dates in shop time', () => {
    expect(formatDay('2026-11-06')).toBe('6 Nov 2026')
    expect(formatWhen('2026-11-06T20:30:00Z')).toBe('7 Nov 2026, 2:00 am')
  })
})
