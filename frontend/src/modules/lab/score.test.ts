import { describe, expect, it } from 'vitest'
import { compare, score } from './score'
import { parseLabel } from '@/modules/scanner/parse-label'

describe('score', () => {
  it('prefers the reading that found a model code, then more fields, then confidence', () => {
    const sideways = { fields: parseLabel(['MRP 750']), lines: [{ text: 'MRP 750', confidence: 0.9 }] }
    const upright = { fields: parseLabel(['IP 1060-11 MP4952']), lines: [{ text: 'x', confidence: 0.5 }] }
    expect(score(upright)).toBeGreaterThan(score(sideways))
  })
})

describe('compare', () => {
  it('checks code, design and MRP against the expected label', () => {
    const fields = parseLabel(['IP 1060-11" MP4952', 'MRP : ₹ 750/-'])
    expect(compare(fields, { code: 'IP1060-11', design_code: 'MP4952', mrp: '750' })).toEqual({ code: true, design: true, mrp: true })
    expect(compare(fields, { code: 'IP1060-11', design_code: null, mrp: null })).toEqual({ code: true, design: false, mrp: false })
  })
})
