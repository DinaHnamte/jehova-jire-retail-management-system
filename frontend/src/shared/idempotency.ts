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

/** A persisted key with the time (ms) it was first issued. */
interface StoredKey extends IssuedKey {
  issuedAt: number
}
type Store = Record<string, StoredKey>

/** Entries older than this are dropped on read. Housekeeping only: a pending attempt is never
 *  reused without the user choosing to retry it, however fresh it is. */
export const PENDING_TTL_MS = 24 * 60 * 60 * 1000

function readStore(storageKey: string): Store {
  try {
    const raw = localStorage.getItem(storageKey)
    if (!raw) return {}
    const v: unknown = JSON.parse(raw)
    if (!v || typeof v !== 'object' || Array.isArray(v)) return {}
    const now = Date.now()
    const out: Store = {}
    let changed = false
    for (const [scope, k] of Object.entries(v as Record<string, unknown>)) {
      const e = k as Partial<StoredKey> | null
      if (!e || typeof e.body !== 'string' || typeof e.value !== 'string') {
        changed = true
        continue
      }
      // Saved before issue times were kept: start its clock now, so it still asks for a choice.
      const issuedAt = typeof e.issuedAt === 'number' ? e.issuedAt : now
      if (issuedAt !== e.issuedAt) changed = true
      if (now - issuedAt > PENDING_TTL_MS) {
        changed = true
        continue
      }
      out[scope] = { body: e.body, value: e.value, issuedAt }
    }
    if (changed) writeStore(storageKey, out)
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
      const prev = s[scope] ?? null
      const issued = nextKey(prev, body)
      s[scope] = issued === prev ? prev : { ...issued, issuedAt: Date.now() }
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

/**
 * One form's attempts at a money action (a payment, a cancel, a return) under `scope`.
 *
 * An attempt that got no answer (maybe only its response was lost) stays pending in
 * localStorage. When the form opens with one pending, `open()` returns its body and the user
 * must choose: `retry()` sends that same body with the same key (the server replays it if it
 * had gone through), `startNew()` forgets it so the next key is new whatever the body. Until
 * they choose, `key()` throws, so a later identical action never silently reuses the old key
 * and gets the old response replayed instead of being recorded.
 */
export function attemptSession<B>(storageKey: string, scope: string) {
  const keys = persistedKeys(storageKey)
  let choosing = false
  const pending = (): B | null => {
    const raw = keys.pendingBody(scope)
    if (raw === null) return null
    try {
      return JSON.parse(raw) as B
    } catch {
      return null
    }
  }
  return {
    /** Call when the form opens: the pending attempt's body, if any. A choice is then required. */
    open(): B | null {
      const p = pending()
      choosing = p !== null
      return p
    },
    needsChoice(): boolean {
      return choosing
    },
    /** Retry the pending attempt: its body, which `key()` then answers with its same key. */
    retry(): B | null {
      choosing = false
      return pending()
    },
    /** Forget the pending attempt: the next key is new. */
    startNew() {
      keys.clear(scope)
      choosing = false
    },
    /** The key for `body`: the same for the same body within this attempt, new for another. */
    key(body: B): string {
      if (choosing) throw new Error('Choose to retry the last attempt or start a new one first.')
      return keys.key(scope, body)
    },
    /** The server answered for good (success or a 4xx): the next attempt gets a new key. */
    done() {
      keys.clear(scope)
    },
  }
}
