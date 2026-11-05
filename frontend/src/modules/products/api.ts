// Products API (backend src/catalog). Money is a two-decimal string, e.g. "750.00".
import { api } from '@/shared/api/client'
import { toApiQuery, type ListParams } from './list-query'

export interface NamedRef {
  id: string
  name: string
}

export interface ProductSummary {
  id: string
  name: string
  code: string | null
  base_code: string | null
  variant: string | null
  selling_price: string
  mrp: string | null
  unit: string
  category: NamedRef | null
  archived: boolean
}

export interface SearchHit extends ProductSummary {
  match: 'exact' | 'base' | 'prefix' | 'name'
}

export interface Identifier {
  id: string
  type: 'product_code' | 'sku' | 'barcode' | 'alias'
  raw_value: string
  normalized_value: string
}

export interface Product extends ProductSummary {
  description: string | null
  brand: NamedRef
  pack_qty: number
  attributes: Record<string, unknown>
  /** Only sent to people who can edit products. */
  default_cost: string | null
  hsn_code: string | null
  identifiers: Identifier[]
  created_at: string
  updated_at: string
}

export interface Page<T> {
  items: T[]
  page: number
  per_page: number
  total: number
}

export interface ProductBody {
  name?: string
  code?: string
  category_id?: string | null
  description?: string | null
  unit?: string
  pack_qty?: number
  selling_price?: string
  mrp?: string | null
  default_cost?: string | null
  hsn_code?: string | null
}

export interface ImportRow {
  row: number
  code: string | null
  name: string
  status: 'create' | 'skip' | 'error'
  errors: { field: string; message: string }[]
}

export interface ImportReport {
  rows: ImportRow[]
  created: number
  skipped: number
  new_categories: string[]
}

export const listProducts = (p: ListParams, signal?: AbortSignal) =>
  api.get<Page<ProductSummary>>(`/products?${toApiQuery(p)}`, { signal })

export const searchProducts = (q: string, signal?: AbortSignal) =>
  api.get<{ results: SearchHit[] }>(`/products/search?q=${encodeURIComponent(q)}`, { signal })

export const getProduct = (id: string) => api.get<Product>(`/products/${id}`)
export const createProduct = (body: ProductBody) => api.post<Product>('/products', body)
export const updateProduct = (id: string, body: ProductBody) => api.patch<Product>(`/products/${id}`, body)
export const archiveProduct = (id: string) => api.post<Product>(`/products/${id}/archive`, {})
export const restoreProduct = (id: string) => api.post<Product>(`/products/${id}/restore`, {})

export const addIdentifier = (id: string, type: 'sku' | 'alias', value: string) =>
  api.post<Identifier>(`/products/${id}/identifiers`, { type, value })
export const removeIdentifier = (id: string, iid: string) => api.delete<void>(`/products/${id}/identifiers/${iid}`)

export const listCategories = () => api.get<NamedRef[]>('/categories')
export const createCategory = (name: string) => api.post<NamedRef>('/categories', { name })

/** Check (`dryRun`) or run a CSV import. A blocked run rejects with the report in `ApiError.details`. */
export const importProducts = (csv: string, dryRun: boolean) =>
  api.postRaw<ImportReport>(`/products/import?dry_run=${dryRun}`, csv, 'text/csv')
