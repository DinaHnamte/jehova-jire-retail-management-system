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
