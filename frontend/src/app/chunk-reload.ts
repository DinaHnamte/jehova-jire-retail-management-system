// After a deploy, an open tab may ask for a code chunk the new release deleted.
// Recognise that failure so the router can reload into the new version.

const PATTERNS = [
  /failed to fetch dynamically imported module/i, // Chrome, Edge
  /error loading dynamically imported module/i, // Firefox
  /importing a module script failed/i, // Safari
  /is not a valid javascript mime type/i, // HTML served for a missing chunk
]

export function isChunkLoadError(e: unknown): boolean {
  return e instanceof Error && PATTERNS.some((p) => p.test(e.message))
}

const FLAG = 'jj-chunk-reload'

/** Reload once to `path`; returns false if we already tried (avoid a reload loop). */
export function reloadOnce(path: string): boolean {
  try {
    if (sessionStorage.getItem(FLAG) === path) return false
    sessionStorage.setItem(FLAG, path)
  } catch {
    // Storage unavailable: still reload once; worst case the user sees the error after.
  }
  window.location.assign(path)
  return true
}

export function clearReloadFlag() {
  try {
    sessionStorage.removeItem(FLAG)
  } catch {
    /* ignore */
  }
}
