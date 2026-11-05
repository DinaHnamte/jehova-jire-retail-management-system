import { afterEach, describe, expect, it, vi } from 'vitest'
import { openWhatsApp } from './whatsapp'

describe('openWhatsApp', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('navigates to the given url', () => {
    const loc = { href: '' }
    vi.stubGlobal('window', { location: loc })
    openWhatsApp('https://wa.me/919999999999?text=hi')
    expect(loc.href).toBe('https://wa.me/919999999999?text=hi')
  })
})
