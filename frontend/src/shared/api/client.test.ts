import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, configureApi } from './client'
import { ApiError } from './errors'

function jsonResponse(status: number, body: unknown) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

async function rejection(p: Promise<unknown>): Promise<ApiError> {
  try {
    await p
  } catch (e) {
    return e as ApiError
  }
  throw new Error('expected the request to fail')
}

describe('api client', () => {
  const fetchMock = vi.fn()
  const onUnauthenticated = vi.fn()

  beforeEach(() => {
    vi.stubGlobal('fetch', fetchMock)
    configureApi({ getCsrfToken: () => 'tok123', onUnauthenticated })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
    fetchMock.mockReset()
    onUnauthenticated.mockReset()
  })

  it('adds csrf header on POST but not GET', async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, { ok: true }))
    await api.get('/auth/me')
    await api.post('/auth/logout')
    const [getUrl, getInit] = fetchMock.mock.calls[0]!
    const [, postInit] = fetchMock.mock.calls[1]!
    expect(getUrl).toBe('/api/v1/auth/me')
    expect(new Headers(getInit.headers).has('x-csrf-token')).toBe(false)
    expect(new Headers(postInit.headers).get('x-csrf-token')).toBe('tok123')
    expect(postInit.credentials).toBe('same-origin')
  })

  it('sends JSON bodies and returns parsed JSON', async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, { a: 1 }))
    const out = await api.post<{ a: number }>('/x', { b: 2 })
    expect(out).toEqual({ a: 1 })
    const [, init] = fetchMock.mock.calls[0]!
    expect(init.body).toBe('{"b":2}')
    expect(new Headers(init.headers).get('content-type')).toBe('application/json')
  })

  it('returns undefined for 204', async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }))
    expect(await api.post('/auth/logout')).toBeUndefined()
  })

  it('parses error shape into ApiError', async () => {
    fetchMock.mockResolvedValue(
      jsonResponse(422, {
        error: { code: 'VALIDATION_FAILED', message: 'Bad name', details: { f: 1 }, request_id: 'r-1' },
      }),
    )
    const err = await rejection(api.post('/x', {}))
    expect(err).toBeInstanceOf(ApiError)
    expect(err.status).toBe(422)
    expect(err.code).toBe('VALIDATION_FAILED')
    expect(err.message).toBe('Bad name')
    expect(err.requestId).toBe('r-1')
  })

  it('calls onUnauthenticated on 401', async () => {
    fetchMock.mockResolvedValue(
      jsonResponse(401, { error: { code: 'UNAUTHENTICATED', message: 'Log in', details: {}, request_id: 'r' } }),
    )
    await expect(api.get('/auth/me')).rejects.toBeInstanceOf(ApiError)
    expect(onUnauthenticated).toHaveBeenCalledTimes(1)
  })

  it('does not call onUnauthenticated for wrong-password 401', async () => {
    fetchMock.mockResolvedValue(
      jsonResponse(401, { error: { code: 'INVALID_CREDENTIALS', message: 'Wrong', details: {}, request_id: 'r' } }),
    )
    await expect(api.post('/auth/login', {})).rejects.toBeInstanceOf(ApiError)
    expect(onUnauthenticated).not.toHaveBeenCalled()
  })

  it('non-JSON 502 becomes ApiError with code NETWORK_ERROR', async () => {
    fetchMock.mockResolvedValue(new Response('<html>Bad Gateway</html>', { status: 502 }))
    const err = await rejection(api.get('/x'))
    expect(err).toBeInstanceOf(ApiError)
    expect(err.code).toBe('NETWORK_ERROR')
    expect(err.status).toBe(502)
  })

  it('fetch failure (offline) becomes NETWORK_ERROR with status 0', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'))
    const err = await rejection(api.get('/x'))
    expect(err.code).toBe('NETWORK_ERROR')
    expect(err.status).toBe(0)
  })

  it('passes the abort signal and rejects with AbortError', async () => {
    const ctrl = new AbortController()
    fetchMock.mockImplementation((_url: string, init: RequestInit) => {
      expect(init.signal).toBe(ctrl.signal)
      return Promise.reject(new DOMException('aborted', 'AbortError'))
    })
    ctrl.abort()
    const err = await rejection(api.get('/products', { signal: ctrl.signal }))
    expect(err).toBeInstanceOf(DOMException)
    expect((err as unknown as DOMException).name).toBe('AbortError')
  })
})
