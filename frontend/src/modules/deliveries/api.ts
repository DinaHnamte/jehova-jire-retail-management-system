// Deliveries API (backend src/deliveries, src/drivers). Money is a two-decimal string; timestamps are ISO strings.
import { api } from '@/shared/api/client'
import type { DeliveryStatus, FailReason } from './labels'

export type Collector = 'shop' | 'driver'
export type DriverPayMethod = 'retain_from_collection' | 'paid_by_shop'
/** The list tabs; `done` is the last 7 days. */
export type DeliveryTab = 'to_send' | 'out' | 'failed' | 'done'

export interface DriverBrief {
  id: string
  name: string
  active: boolean
}

export interface DeliveryListItem {
  id: string
  order_id: string
  order_number: string
  status: DeliveryStatus
  recipient_name: string
  locality: string
  /** The current driver, else the last one assigned; null when never assigned. */
  driver: DriverBrief | null
  amount_to_collect: string
  fee: string
  /** When the status last changed. */
  since: string
  /** The latest attempt's reason, for a failed delivery. */
  fail_reason: FailReason | null
}

export interface DeliveryCounts {
  to_send: number
  out: number
  failed: number
}

export interface DeliveryList {
  items: DeliveryListItem[]
  counts: DeliveryCounts
}

export interface DeliveryOrderMoney {
  id: string
  number: string
  /** Goods plus the delivery's current fee. */
  total: string
  paid: string
  owed: string
}

export interface DeliveryItem {
  /** What check-in lists each line by. */
  order_line_id: string
  name: string
  code: string | null
  quantity: number
}

export interface DeliveryDriver {
  id: string
  name: string
  phone: string
  active: boolean
}

export interface DeliveryEvent {
  from: DeliveryStatus | null
  to: DeliveryStatus
  /** `status`, `edit`, ... */
  kind: string
  actor_name: string
  note: string | null
  /** For an edit: `{ field: [old, new] }`. */
  changes: Record<string, [unknown, unknown]> | null
  at: string
}

export interface DeliveryAttempt {
  driver_name: string
  reason: FailReason
  note: string | null
  trip_pay: string | null
  at: string
}

export interface DeliveryDetail {
  id: string
  order_id: string
  status: DeliveryStatus
  recipient_name: string
  recipient_phone: string
  address_line: string
  locality: string
  landmark: string | null
  instructions: string | null
  fee: string
  driver_pay: string | null
  collector: Collector
  pay_method: DriverPayMethod
  received_by: string | null
  created_at: string
  assigned_at: string | null
  picked_up_at: string | null
  delivered_at: string | null
  closed_at: string | null
  updated_at: string
  order: DeliveryOrderMoney
  items: DeliveryItem[]
  driver: DeliveryDriver | null
  /** Oldest first. */
  events: DeliveryEvent[]
  /** Oldest first. */
  attempts: DeliveryAttempt[]
  amount_to_collect: string
  /** Both null when no driver has the job. */
  driver_whatsapp_url: string | null
  customer_whatsapp_url: string | null
  maps_url: string
}

// Request bodies.
export interface AssignBody {
  driver_id: string
  driver_pay?: string
}

/** PATCH before pickup. `null` clears landmark, instructions or driver_pay. */
export interface EditBody {
  recipient_name?: string
  recipient_phone?: string
  address_line?: string
  locality?: string
  landmark?: string | null
  instructions?: string | null
  fee?: string
  driver_pay?: string | null
  collector?: Collector
  pay_method?: DriverPayMethod
}

export interface StatusBody {
  to: 'picked_up' | 'delivered' | 'failed'
  /** Required for `delivered`. */
  received_by?: string
  /** Required for `failed`. */
  reason?: FailReason
  /** Required when the reason is `other`. */
  note?: string
}

export interface RetryBody {
  driver_id: string
  driver_pay?: string
  /** Pay for the failed trip; 0 when left out. */
  trip_pay?: string
}

export interface CancelDeliveryBody {
  reason?: string
}

export interface CheckInBody {
  lines: { order_line_id: string; disposition: 'sellable' | 'damaged' }[]
  trip_pay?: string
}

// Driver's own jobs (`/me/deliveries`): no prices, costs or customers.
export interface DriverActiveItem {
  id: string
  order_number: string
  status: DeliveryStatus
  locality: string
  recipient_name: string
  amount_to_collect: string
}

/** A finished job: no address, phone or items. */
export interface DriverHistoryItem {
  id: string
  order_number: string
  locality: string
  at: string
  amount: string
  status: DeliveryStatus
}

export interface MyJobs {
  active: DriverActiveItem[]
  /** Newest 20. */
  history: DriverHistoryItem[]
}

export interface JobItem {
  name: string
  code: string | null
  quantity: number
}

export interface JobDetail {
  id: string
  order_number: string
  status: DeliveryStatus
  recipient_name: string
  recipient_phone: string
  address_line: string
  locality: string
  landmark: string | null
  instructions: string | null
  items: JobItem[]
  amount_to_collect: string
  collector: Collector
  pay_method: DriverPayMethod
  driver_pay: string | null
  maps_url: string
}

/** The full job while it is the driver's, the history fields otherwise. Tell them apart with `isActiveJob`. */
export type MyJob = JobDetail | DriverHistoryItem

export const isActiveJob = (j: MyJob): j is JobDetail => 'address_line' in j

/** Row of `GET /drivers` (needs `drivers.manage`). Shared by Tasks 11 and 13. */
export interface Driver {
  id: string
  display_name: string
  username: string
  phone: string
  default_pay: string
  note: string | null
  is_active: boolean
  open_jobs: number
}

const idem = (key: string) => ({ headers: { 'Idempotency-Key': key } })

export const listDeliveries = (tab: DeliveryTab = 'to_send') => api.get<DeliveryList>(`/deliveries?status=${tab}`)
export const getDelivery = (id: string) => api.get<DeliveryDetail>(`/deliveries/${id}`)
export const assign = (id: string, body: AssignBody) => api.post<DeliveryDetail>(`/deliveries/${id}/assign`, body)
export const editDelivery = (id: string, body: EditBody, key: string) =>
  api.patch<DeliveryDetail>(`/deliveries/${id}`, body, idem(key))
export const setStatus = (id: string, body: StatusBody) => api.post<DeliveryDetail>(`/deliveries/${id}/status`, body)
export const retry = (id: string, body: RetryBody) => api.post<DeliveryDetail>(`/deliveries/${id}/retry`, body)
export const cancelDelivery = (id: string, body: CancelDeliveryBody, key: string) =>
  api.post<DeliveryDetail>(`/deliveries/${id}/cancel`, body, idem(key))
export const checkIn = (id: string, body: CheckInBody, key: string) =>
  api.post<DeliveryDetail>(`/deliveries/${id}/check-in`, body, idem(key))

export const myJobs = () => api.get<MyJobs>('/me/deliveries')
export const myJob = (id: string) => api.get<MyJob>(`/me/deliveries/${id}`)
export const setMyStatus = (id: string, body: StatusBody) => api.post<MyJob>(`/me/deliveries/${id}/status`, body)

/** Drivers for the assign dialog (Task 11) and the drivers page (Task 13). Needs `drivers.manage`. */
export const listDrivers = () => api.get<Driver[]>('/drivers')
