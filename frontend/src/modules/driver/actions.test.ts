import { describe, expect, it } from 'vitest'
import { ApiError } from '@/shared/api/errors'
import { canFail, primaryAction, statusConflictIsDone } from './actions'

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
