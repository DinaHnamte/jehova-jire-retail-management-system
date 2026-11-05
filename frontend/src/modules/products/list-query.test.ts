import { describe, expect, it } from 'vitest'
import { DEFAULT_LIST, fromRouteQuery, toApiQuery, toRouteQuery } from './list-query'

describe('list query', () => {
  it('reads values from the route', () => {
    const p = fromRouteQuery({ page: '3', status: 'archived', q: 'plate', category: 'c1', perPage: '50' })
    expect(p).toEqual({ q: 'plate', status: 'archived', categoryId: 'c1', page: 3, perPage: 50 })
  })

  it('falls back to defaults for bad values', () => {
    expect(fromRouteQuery({ page: '-2', perPage: 'x', status: 'bogus' })).toEqual(DEFAULT_LIST)
    expect(fromRouteQuery({ perPage: '1000' }).perPage).toBe(100)
    expect(fromRouteQuery({ page: ['2', '3'] }).page).toBe(2)
  })

  it('omits defaults from the URL and round-trips', () => {
    expect(toRouteQuery(DEFAULT_LIST)).toEqual({})
    const p = { q: 'bowl', status: 'all' as const, categoryId: 'c9', page: 4, perPage: 25 }
    expect(fromRouteQuery(toRouteQuery(p))).toEqual(p)
  })

  it('builds the API query string', () => {
    expect(toApiQuery({ ...DEFAULT_LIST, q: 'IP 1060', page: 2 })).toBe('q=IP+1060&status=active&page=2&per_page=25')
    expect(toApiQuery({ ...DEFAULT_LIST, categoryId: 'c1' })).toBe('status=active&category_id=c1&page=1&per_page=25')
  })
})
