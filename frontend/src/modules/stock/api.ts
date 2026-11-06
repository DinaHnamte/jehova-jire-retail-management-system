// Inventory API (backend src/inventory). Money is a decimal string ("120.50"); average cost
// has four decimals ("110.0000"). Quantities are whole numbers in the product's base unit.
import { api } from '@/shared/api/client'
import type { NamedRef, Page, ProductSummary } from '@/modules/products/api'

export type MovementType = 'opening_balance' | 'purchase_receipt' | 'adjustment_in' | 'adjustment_out' | 'damage' | 'sale' | 'sale_cancel' | 'sale_return'
export type AdjustType = 'adjustment_in' | 'adjustment_out' | 'damage'
export type StockFilter = 'out' | 'unknown_cost'

export interface StockRow {
  product: ProductSummary
  on_hand: number
  avg_cost: string | null
}

export interface ProductStock {
  on_hand: number
  avg_cost: string | null
  stock_value: string | null
}

export interface Movement {
  id: string
  movement_type: MovementType
  quantity: number
  unit_cost: string | null
  reason: string | null
  source_type: 'goods_receipt' | 'stock_count' | 'adjustment' | 'order' | 'return'
  source_id: string
  created_by_name: string
  created_at: string
}

export interface Supplier {
  id: string
  name: string
  phone: string | null
  notes: string | null
  archived: boolean
}

export interface SupplierBody {
  name?: string
  phone?: string | null
  notes?: string | null
  archived?: boolean
}

export interface ReceiptLineIn {
  product_id: string
  quantity: number
  unit_cost: string
}

export interface ReceiptLine {
  product_id: string
  code: string | null
  name: string
  quantity: number
  unit_cost: string
  line_total: string
}

export interface Receipt {
  id: string
  supplier: NamedRef
  received_on: string
  note: string | null
  status: 'draft' | 'confirmed'
  lines: ReceiptLine[]
  total: string
  confirmed_at: string | null
  confirmed_by_name: string | null
}

export interface ReceiptSummary {
  id: string
  supplier: NamedRef
  received_on: string
  status: 'draft' | 'confirmed'
  line_count: number
  total: string
}

export interface CountLineIn {
  product_id: string
  counted: number
  unit_cost: string | null
}

export interface CountLine {
  product_id: string
  code: string | null
  name: string
  counted: number
  unit_cost: string | null
  on_hand_now: number
  expected: number | null
  difference: number
  changed_since_start: boolean
  /** No stock was ever recorded for this product, so this count is its opening balance. */
  first_count: boolean
}

export interface StockCount {
  id: string
  note: string | null
  status: 'draft' | 'posted'
  created_at: string
  posted_at: string | null
  posted_by_name: string | null
  lines: CountLine[]
}

export interface CountSummary {
  id: string
  note: string | null
  status: 'draft' | 'posted'
  created_at: string
  posted_at: string | null
  line_count: number
}

export interface StockMismatch {
  kind: 'stock'
  product_id: string
  code: string | null
  name: string
  balance: number
  ledger_sum: number
}

export interface LedgerUnbalanced {
  kind: 'ledger_unbalanced'
  transaction_id: string
}

export interface ReceivableOutOfRange {
  kind: 'receivable_out_of_range'
  order_id: string
  owed: string
}

export type Mismatch = StockMismatch | LedgerUnbalanced | ReceivableOutOfRange

export interface CheckRun {
  id: string
  ran_at: string
  mismatches: Mismatch[]
}

function qs(params: Record<string, string | number | null | undefined>): string {
  const s = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== null && v !== undefined && v !== '') s.set(k, String(v))
  const out = s.toString()
  return out ? `?${out}` : ''
}

// Stock
export const listStock = (p: { q?: string; filter?: StockFilter | null; page?: number }, signal?: AbortSignal) =>
  api.get<Page<StockRow>>(`/stock${qs({ q: p.q?.trim(), filter: p.filter, page: p.page })}`, { signal })
export const getProductStock = (id: string) => api.get<ProductStock>(`/products/${id}/stock`)
export const listMovements = (id: string, page = 1, perPage = 10) =>
  api.get<Page<Movement>>(`/products/${id}/movements${qs({ page, per_page: perPage })}`)
export const adjustStock = (id: string, body: { type: AdjustType; quantity: number; reason: string }) =>
  api.post<ProductStock>(`/products/${id}/adjustments`, body)

// Suppliers
export const listSuppliers = (p: { q?: string; includeArchived?: boolean; page?: number } = {}) =>
  api.get<Page<Supplier>>(
    `/suppliers${qs({ q: p.q?.trim(), include_archived: p.includeArchived ? 'true' : null, page: p.page })}`,
  )
export const createSupplier = (body: SupplierBody) => api.post<Supplier>('/suppliers', body)
export const updateSupplier = (id: string, body: SupplierBody) => api.patch<Supplier>(`/suppliers/${id}`, body)

// Goods receipts
export const listReceipts = (page = 1) => api.get<Page<ReceiptSummary>>(`/goods-receipts${qs({ page })}`)
export const createReceipt = (body: { supplier_id: string; received_on?: string; note?: string | null }) =>
  api.post<Receipt>('/goods-receipts', body)
export const getReceipt = (id: string) => api.get<Receipt>(`/goods-receipts/${id}`)
export const updateReceipt = (id: string, body: { supplier_id?: string; received_on?: string; note?: string | null }) =>
  api.patch<Receipt>(`/goods-receipts/${id}`, body)
export const deleteReceipt = (id: string) => api.delete<void>(`/goods-receipts/${id}`)
export const putReceiptLines = (id: string, lines: ReceiptLineIn[]) =>
  api.put<Receipt>(`/goods-receipts/${id}/lines`, { lines })
export const confirmReceipt = (id: string) => api.post<Receipt>(`/goods-receipts/${id}/confirm`, {})

// Stock counts
export const listCounts = (page = 1) => api.get<Page<CountSummary>>(`/stock-counts${qs({ page })}`)
export const createCount = (note: string | null) => api.post<StockCount>('/stock-counts', { note })
export const getCount = (id: string) => api.get<StockCount>(`/stock-counts/${id}`)
export const updateCount = (id: string, note: string | null) => api.patch<StockCount>(`/stock-counts/${id}`, { note })
export const deleteCount = (id: string) => api.delete<void>(`/stock-counts/${id}`)
export const putCountLines = (id: string, lines: CountLineIn[]) =>
  api.put<StockCount>(`/stock-counts/${id}/lines`, { lines })
export const postCount = (id: string) => api.post<StockCount>(`/stock-counts/${id}/post`, {})

// Stock check
export const latestCheck = () => api.get<CheckRun | null>('/stock-checks/latest')
export const runCheck = () => api.post<CheckRun>('/stock-checks', {})
