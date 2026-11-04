import { describe, expect, it } from 'vitest'
import { isChunkLoadError } from './chunk-reload'

describe('isChunkLoadError', () => {
  it('recognises failed dynamic imports from each browser', () => {
    expect(isChunkLoadError(new TypeError('Failed to fetch dynamically imported module: /assets/x.js'))).toBe(true)
    expect(isChunkLoadError(new TypeError('error loading dynamically imported module'))).toBe(true)
    expect(isChunkLoadError(new TypeError('Importing a module script failed.'))).toBe(true)
    expect(isChunkLoadError(new Error("'text/html' is not a valid JavaScript MIME type."))).toBe(true)
  })

  it('ignores ordinary errors', () => {
    expect(isChunkLoadError(new Error('Wrong username or password.'))).toBe(false)
    expect(isChunkLoadError('nope')).toBe(false)
  })
})
