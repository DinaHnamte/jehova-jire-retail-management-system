import { describe, expect, it } from 'vitest'
import cases from '../../../fixtures/codes.json'
import { normalizeCode, normalizeQuery } from './code'

describe('normalizeCode (shared fixtures)', () => {
  it('has the shared cases', () => expect(cases.length).toBeGreaterThanOrEqual(11))

  for (const c of cases) {
    it(`handles ${JSON.stringify(c.raw)}`, () => {
      const got = normalizeCode(c.raw)
      if (c.normalized === null) {
        expect(got).toBeNull()
      } else {
        expect(got).toEqual({ normalized: c.normalized, base: c.base, variant: c.variant, digits: c.digits })
      }
    })
  }
})

describe('normalizeQuery', () => {
  it('never fails', () => {
    expect(normalizeQuery(' ip 1060 – 11″ ')).toBe('IP1060-11')
    expect(normalizeQuery('   ')).toBe('')
  })
})
