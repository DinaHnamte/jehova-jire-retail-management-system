// Products list state lives in the URL query, so Back and shared links restore it.
import type { LocationQuery, LocationQueryValue } from 'vue-router'

export type Status = 'active' | 'archived' | 'all'

export interface ListParams {
  q: string
  status: Status
  categoryId: string | null
  page: number
  perPage: number
}

export const DEFAULT_LIST: Readonly<ListParams> = Object.freeze({
  q: '',
  status: 'active',
  categoryId: null,
  page: 1,
  perPage: 25,
})

const STATUSES: Status[] = ['active', 'archived', 'all']
const MAX_PER_PAGE = 100

function first(v: LocationQueryValue | LocationQueryValue[] | undefined): string | null {
  return (Array.isArray(v) ? v[0] : v) ?? null
}

function positiveInt(v: string | null, fallback: number, max = Infinity): number {
  const n = v === null ? NaN : Number(v)
  return Number.isInteger(n) && n >= 1 ? Math.min(n, max) : fallback
}

export function fromRouteQuery(query: LocationQuery): ListParams {
  const status = first(query.status)
  return {
    q: first(query.q) ?? DEFAULT_LIST.q,
    status: STATUSES.includes(status as Status) ? (status as Status) : DEFAULT_LIST.status,
    categoryId: first(query.category) || DEFAULT_LIST.categoryId,
    page: positiveInt(first(query.page), DEFAULT_LIST.page),
    perPage: positiveInt(first(query.perPage), DEFAULT_LIST.perPage, MAX_PER_PAGE),
  }
}

/** URL query for these params, leaving out anything at its default. */
export function toRouteQuery(p: ListParams): Record<string, string> {
  const out: Record<string, string> = {}
  if (p.q) out.q = p.q
  if (p.status !== DEFAULT_LIST.status) out.status = p.status
  if (p.categoryId) out.category = p.categoryId
  if (p.page !== DEFAULT_LIST.page) out.page = String(p.page)
  if (p.perPage !== DEFAULT_LIST.perPage) out.perPage = String(p.perPage)
  return out
}

/** Query string for `GET /products`. */
export function toApiQuery(p: ListParams): string {
  const s = new URLSearchParams()
  if (p.q.trim()) s.set('q', p.q.trim())
  s.set('status', p.status)
  if (p.categoryId) s.set('category_id', p.categoryId)
  s.set('page', String(p.page))
  s.set('per_page', String(p.perPage))
  return s.toString()
}
