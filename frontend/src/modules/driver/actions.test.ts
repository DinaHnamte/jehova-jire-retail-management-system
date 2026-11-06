import { describe, expect, it } from 'vitest'
import { ApiError } from '@/shared/api/errors'
import { canFail, collectLabel, primaryAction, statusConflictIsDone } from './actions'

describe('primaryAction', () => {
  it('is the next step for the driver', () => {
    expect(primaryAction('assigned')).toBe('picked_up')
    expect(primaryAction('picked_up')).toBe('delivered')
  })
  it('is nothing for any other status', () => {
    for (const s of ['to_send', 'delivered', 'failed', 'returned', 'cancelled']) expect(primaryAction(s as never)).toBeNull()
  })
})

describe('canFail', () => {
  it('is only possible once picked up', () => {
    expect(canFail('picked_up')).toBe(true)
    for (const s of ['to_send', 'assigned', 'delivered', 'failed', 'returned', 'cancelled']) expect(canFail(s as never)).toBe(false)
  })
})

describe('statusConflictIsDone', () => {
  const conflict = (status: string, code = 'DELIVERY_STATUS', http = 409) => new ApiError(http, code, 'x', { status })
  it('is true when the delivery is already in the requested status', () => {
    expect(statusConflictIsDone(conflict('delivered'), 'delivered')).toBe(true)
  })
  it('is false for another status, another code or another HTTP status', () => {
    expect(statusConflictIsDone(conflict('picked_up'), 'delivered')).toBe(false)
    expect(statusConflictIsDone(conflict('delivered', 'OTHER'), 'delivered')).toBe(false)
    expect(statusConflictIsDone(conflict('delivered', 'DELIVERY_STATUS', 400), 'delivered')).toBe(false)
    expect(statusConflictIsDone(new ApiError(409, 'DELIVERY_STATUS', 'x'), 'delivered')).toBe(false)
  })
})

describe('collectLabel', () => {
  it('tells the driver what to collect', () => {
    expect(collectLabel({ collector: 'driver', amount_to_collect: '130.00' })).toBe('Collect ₹130.00')
  })
  it('says nothing to collect when the shop collects', () => {
    expect(collectLabel({ collector: 'shop', amount_to_collect: '0.00' })).toBe('Nothing to collect')
  })
  it('says already paid when the driver collects and nothing is owed', () => {
    expect(collectLabel({ collector: 'driver', amount_to_collect: '0.00' })).toBe('Already paid')
  })
})
