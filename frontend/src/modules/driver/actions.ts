import type { ApiError } from '@/shared/api/errors'
import type { DeliveryStatus } from '@/modules/deliveries/labels'
import { conflictStatus, isStatusConflict } from '@/modules/deliveries/errors'

/** The one step the driver takes next, or null when the job is not theirs to move. */
export function primaryAction(status: DeliveryStatus): 'picked_up' | 'delivered' | null {
  if (status === 'assigned') return 'picked_up'
  if (status === 'picked_up') return 'delivered'
  return null
}

/** "Couldn't deliver" is only offered once the goods are out with the driver. */
export const canFail = (status: DeliveryStatus): boolean => status === 'picked_up'

/** Ruling 8: a 409 DELIVERY_STATUS already in the requested status means the step is done (a double tap). */
export function statusConflictIsDone(err: ApiError, requested: string): boolean {
  return isStatusConflict(err) && err.code === 'DELIVERY_STATUS' && conflictStatus(err) === requested
}
