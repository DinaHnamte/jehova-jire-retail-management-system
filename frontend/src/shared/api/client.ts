import { ApiError } from './errors'

const BASE = '/api/v1'
const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS'])

interface ApiConfig {
  getCsrfToken: () => string | null
  /** Called when a request fails because the session is missing or expired. */
  onUnauthenticated: () => void
}

let config: ApiConfig = { getCsrfToken: () => null, onUnauthenticated: () => {} }

export function configureApi(c: ApiConfig) {
  config = c
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers = new Headers({ accept: 'application/json' })
  if (body !== undefined) headers.set('content-type', 'application/json')
  if (!SAFE_METHODS.has(method)) {
    const token = config.getCsrfToken()
    if (token) headers.set('x-csrf-token', token)
  }

  let res: Response
  try {
    res = await fetch(BASE + path, {
      method,
      headers,
      credentials: 'same-origin',
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiError(0, 'NETWORK_ERROR', 'Network request failed')
  }

  if (res.status === 204) return undefined as T

  const isJson = res.headers.get('content-type')?.includes('application/json')
  const data: unknown = isJson ? await res.json().catch(() => null) : null

  if (!res.ok) {
    const e = (data as { error?: { code?: string; message?: string; details?: unknown; request_id?: string } } | null)
      ?.error
    if (!e?.code) throw new ApiError(res.status, 'NETWORK_ERROR', `Server returned ${res.status}`)
    const err = new ApiError(res.status, e.code, e.message ?? e.code, e.details ?? {}, e.request_id)
    if (err.code === 'UNAUTHENTICATED') config.onUnauthenticated()
    throw err
  }
  return data as T
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body),
  patch: <T>(path: string, body?: unknown) => request<T>('PATCH', path, body),
  delete: <T>(path: string) => request<T>('DELETE', path),
}
