// Driver accounts (backend src/drivers). All calls need `drivers.manage`.
import { api } from '@/shared/api/client'
import { ApiError } from '@/shared/api/errors'
import { listDrivers, type Driver } from '@/modules/deliveries/api'

export { listDrivers }
export type { Driver }

/** Client-side hint only; the server's rule (backend MIN_PASSWORD_LEN) is authoritative. */
export const MIN_PASSWORD_LEN = 12

export interface NewDriver {
  display_name: string
  username: string
  password: string
  phone: string
  default_pay: string
  note?: string
}

export interface DriverFields {
  display_name: string
  phone: string
  default_pay: string
  note: string | null
}

export type DriverPatch = Partial<DriverFields & { is_active: boolean }>

/** Only the fields that differ from the saved driver; an empty note is sent as null. */
export function buildDriverPatch(saved: Driver, edited: DriverFields): DriverPatch {
  const patch: DriverPatch = {}
  if (edited.display_name.trim() !== saved.display_name) patch.display_name = edited.display_name.trim()
  if (edited.phone.trim() !== saved.phone) patch.phone = edited.phone.trim()
  if (edited.default_pay !== saved.default_pay) patch.default_pay = edited.default_pay
  const note = edited.note?.trim() || null
  if (note !== (saved.note?.trim() || null)) patch.note = note
  return patch
}

/** The form field a server validation error belongs to, if it names one. */
export function errorField(e: unknown): string | null {
  if (!(e instanceof ApiError)) return null
  const f = (e.details as { field?: unknown } | null)?.field
  return typeof f === 'string' ? f : null
}

export const createDriver = (body: NewDriver) => api.post<Driver>('/drivers', body)
export const updateDriver = (id: string, body: DriverPatch) => api.patch<Driver>(`/drivers/${id}`, body)
export const resetDriverPassword = (id: string, password: string) =>
  api.post<void>(`/drivers/${id}/password`, { password })
