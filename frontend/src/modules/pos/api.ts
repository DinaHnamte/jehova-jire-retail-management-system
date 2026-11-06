// POS API (backend src/orders and src/customers). Money is a two-decimal string, e.g. "750.00".
// Timestamps are ISO strings.
import { api } from '@/shared/api/client'
import type { Page } from '@/modules/products/api'

export type PaymentStatus = 'paid' | 'partly_paid' | 'unpaid'
export type PayMethod = 'cash' | 'upi'

export interface Customer {
  id: string
  phone: string
  name: string | null
  note: string | null
  created_at: string
}

/** The customer fields sit at the top level next to `owes` (the server flattens them). */
export interface CustomerRow extends Customer {
  owes: string
  last_sale_at: string | null
}

export interface OrderLine {
  product_id: string
  product_name: string
  product_code: string | null
  quantity: number
  list_price: string
  unit_price: string
  line_discount: string
  order_discount_share: string
  /** After this line's share of the order discount. */
  net_amount: string
  /** Only present for users who can update products. */
  unit_cost?: string
}

export interface PaymentRow {
  id: string
  method: string
  amount: string
  occurred_at: string
}

export interface OrderDetail {
  id: string
  number: string
  status: string
  created_at: string
  customer: Customer | null
  lines: OrderLine[]
  subtotal: string
  order_discount: string
  total: string
  paid: string
  owed: string
  payment_status: PaymentStatus
  payments: PaymentRow[]
  note: string | null
  receipt_url: string
  whatsapp_url: string | null
}

export interface OrderSummary {
  id: string
  number: string
  status: string
  created_at: string
  customer_id: string | null
  customer_name: string | null
  customer_phone: string | null
  total: string
  paid: string
  owed: string
  payment_status: PaymentStatus
}

export interface CustomerDetail {
  customer: Customer
  owes: string
  /** Latest 50, newest first. */
  orders: OrderSummary[]
  /** Latest 50, newest first; each amount is the whole payment. */
  payments: PaymentRow[]
  reminder_url: string | null
}

export interface DailySummary {
  date: string
  sales_count: number
  sales_total: string
  discounts: string
  cash_received: string
  upi_received: string
  /** Cash + UPI taken that day against that day's sales. */
  paid_on_these_sales: string
  /** Cash + UPI taken that day against sales from earlier days. */
  received_on_older_sales: string
  new_owed: string
  total_owed: string
}

export interface ReceiptLine {
  name: string
  code: string | null
  quantity: number
  unit_price: string
  list_price: string
  net_amount: string
}

export interface PublicReceipt {
  shop_name: string
  number: string
  created_at: string
  customer_name: string | null
  lines: ReceiptLine[]
  subtotal: string
  order_discount: string
  total: string
  paid: string
  owed: string
  cancelled: boolean
}

export interface CheckoutLine {
  product_id: string
  quantity: number
  unit_price: string
  line_discount: string
}

export interface CheckoutPayment {
  method: PayMethod
  amount: string
}

export interface CheckoutBody {
  lines: CheckoutLine[]
  order_discount: string
  customer_id?: string
  new_customer?: { phone: string; name?: string }
  payments: CheckoutPayment[]
  note?: string
}

export interface CustomerPaymentBody {
  method: PayMethod
  amount: string
  note?: string
}

export interface CustomerPaymentResult {
  transaction_id: string
  applied: { order_id: string; number: string; amount: string }[]
  owes: string
}

export interface OrderQuery {
  from?: string
  to?: string
  paid?: 'paid' | 'partly_paid' | 'unpaid'
  customer_id?: string
  page?: number
  per_page?: number
}

export interface CustomerBody {
  phone?: string
  name?: string | null
  note?: string | null
}

function qs(params: Record<string, string | number | undefined>): string {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== '') p.set(k, String(v))
  const s = p.toString()
  return s ? `?${s}` : ''
}

const idem = (key: string) => ({ headers: { 'Idempotency-Key': key } })

export const checkout = (body: CheckoutBody, key: string) =>
  api.post<OrderDetail>('/orders/checkout', body, idem(key))
export const getOrder = (id: string) => api.get<OrderDetail>(`/orders/${id}`)
export const listOrders = (q: OrderQuery = {}) => api.get<Page<OrderSummary>>(`/orders${qs({ ...q })}`)

export const searchCustomers = (q: string, signal?: AbortSignal) =>
  api.get<Page<CustomerRow>>(`/customers${qs({ q })}`, { signal })
export const createCustomer = (b: { phone: string; name?: string; note?: string }) =>
  api.post<Customer>('/customers', b)
export const getCustomer = (id: string) => api.get<CustomerDetail>(`/customers/${id}`)
export const updateCustomer = (id: string, b: CustomerBody) => api.patch<Customer>(`/customers/${id}`, b)
export const recordPayment = (id: string, b: CustomerPaymentBody, key: string) =>
  api.post<CustomerPaymentResult>(`/customers/${id}/payments`, b, idem(key))

export const dailySummary = (date?: string) => api.get<DailySummary>(`/reports/daily${qs({ date })}`)
export const publicReceipt = (token: string) =>
  api.get<PublicReceipt>(`/public/receipts/${encodeURIComponent(token)}`)
