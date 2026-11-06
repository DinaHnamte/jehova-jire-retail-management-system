// Idempotency keys for cancelling a sale or taking a return, kept in localStorage so a retry
// after a lost response, or after a reload, sends the same key with the same body.
import { nextKey, type IssuedKey } from '@/shared/idempotency'

const STORAGE_KEY = 'jj.undo.v1'

type Store = Record<string, IssuedKey>

function read(): Store {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const v: unknown = JSON.parse(raw)
    if (!v || typeof v !== 'object' || Array.isArray(v)) return {}
    const out: Store = {}
    for (const [scope, k] of Object.entries(v as Record<string, unknown>)) {
      const e = k as Partial<IssuedKey> | null
      if (e && typeof e.body === 'string' && typeof e.value === 'string') out[scope] = { body: e.body, value: e.value }
    }
    return out
  } catch {
    return {}
  }
}

function write(s: Store) {
  try {
    if (Object.keys(s).length === 0) localStorage.removeItem(STORAGE_KEY)
    else localStorage.setItem(STORAGE_KEY, JSON.stringify(s))
  } catch {
    // Storage blocked: keys just won't survive a reload.
  }
}

/** The key for `body` under `scope`: unchanged for the same body, new for a different one. */
export function pendingUndoKey(scope: string, body: unknown): string {
  const s = read()
  const issued = nextKey(s[scope] ?? null, body)
  s[scope] = issued
  write(s)
  return issued.value
}

/** Forget the key once the request has succeeded. */
export function clearUndoKey(scope: string) {
  const s = read()
  delete s[scope]
  write(s)
}
