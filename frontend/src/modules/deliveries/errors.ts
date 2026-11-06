import { ApiError, friendlyMessage } from '@/shared/api/errors'

/** A 409 that says the delivery is no longer in the status this action expected. */
export function isStatusConflict(e: unknown): e is ApiError {
  return e instanceof ApiError && e.status === 409 && (e.code === 'DELIVERY_STATUS' || e.code === 'DELIVERY_OUT')
}

/** The delivery's current status named in a 409's details, if any. */
export function conflictStatus(e: ApiError): string | null {
  const s = (e.details as { status?: unknown } | null)?.status
  return typeof s === 'string' ? s : null
}

export { friendlyMessage }
