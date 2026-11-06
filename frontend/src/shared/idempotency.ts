// Idempotency keys that belong to one exact request body. The same body (a retry after a
// dropped response) gets the same key; any change, including changing back to an earlier
// body, gets a fresh random key, so the server never sees one key with two different bodies.

export interface IssuedKey {
  /** The exact serialized body this key was issued for. */
  body: string
  value: string
}

/** Reuse `prev` when it was issued for this same `body`, otherwise issue a new key. */
export function nextKey(prev: IssuedKey | null, body: unknown): IssuedKey {
  const s = JSON.stringify(body)
  return prev && prev.body === s ? prev : { body: s, value: crypto.randomUUID() }
}

/** In-memory keyed helper: `key(body)` returns the key for the current body. */
export function makeKeyed() {
  let issued: IssuedKey | null = null
  return {
    key(body: unknown): string {
      issued = nextKey(issued, body)
      return issued.value
    },
  }
}

type Store = Record<string, IssuedKey>

function readStore(storageKey: string): Store {
  try {
    const raw = localStorage.getItem(storageKey)
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

function writeStore(storageKey: string, s: Store) {
  try {
    if (Object.keys(s).length === 0) localStorage.removeItem(storageKey)
    else localStorage.setItem(storageKey, JSON.stringify(s))
  } catch {
    // Storage blocked: keys just won't survive a reload.
  }
}

/** Keys kept in localStorage under `storageKey`, one per scope, so a retry survives a reload. */
export function persistedKeys(storageKey: string) {
  return {
    /** The key for `body` under `scope`: unchanged for the same body, new for a different one. */
    key(scope: string, body: unknown): string {
      const s = readStore(storageKey)
      const issued = nextKey(s[scope] ?? null, body)
      s[scope] = issued
      writeStore(storageKey, s)
      return issued.value
    },
    /** The serialized body last issued a key under `scope`, if any. */
    pendingBody(scope: string): string | null {
      return readStore(storageKey)[scope]?.body ?? null
    },
    clear(scope: string) {
      const s = readStore(storageKey)
      delete s[scope]
      writeStore(storageKey, s)
    },
  }
}
