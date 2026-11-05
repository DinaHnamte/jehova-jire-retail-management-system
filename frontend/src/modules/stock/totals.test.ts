import { describe, expect, it } from 'vitest'
import { countDifference, fieldToLineIndex, grandTotal, lineTotal, reviewTone } from './totals'

describe('lineTotal', () => {
  it('multiplies in whole paise, no float error', () => {
    expect(lineTotal('10', '120.50')).toBe('1205.00')
    expect(lineTotal('3', '0.10')).toBe('0.30')
    expect(lineTotal('7', '19.99')).toBe('139.93')
  })
  it('is null for a bad quantity or cost', () => {
    expect(lineTotal('0', '5')).toBeNull()
    expect(lineTotal('-1', '5')).toBeNull()
    expect(lineTotal('1.5', '5')).toBeNull()
    expect(lineTotal('2', 'abc')).toBeNull()
    expect(lineTotal('2', '1.234')).toBeNull()
  })
})

describe('grandTotal', () => {
  it('adds valid lines and counts invalid ones as zero', () => {
    expect(
      grandTotal([
        { quantity: '10', unit_cost: '120.50' },
        { quantity: '3', unit_cost: '80' },
      ]),
    ).toBe('1445.00')
    expect(grandTotal([{ quantity: '', unit_cost: '5' }])).toBe('0.00')
    expect(grandTotal([])).toBe('0.00')
  })
})

describe('countDifference', () => {
  it('is counted minus on hand, or null while empty', () => {
    expect(countDifference('10', 12)).toBe(-2)
    expect(countDifference('15', 12)).toBe(3)
    expect(countDifference('', 12)).toBeNull()
    expect(countDifference('x', 12)).toBeNull()
  })
})

describe('fieldToLineIndex', () => {
  it('reads the line and key from an API field path', () => {
    expect(fieldToLineIndex('lines[2].unit_cost')).toEqual({ index: 2, key: 'unit_cost' })
    expect(fieldToLineIndex('lines[0].quantity')).toEqual({ index: 0, key: 'quantity' })
    expect(fieldToLineIndex('name')).toBeNull()
    expect(fieldToLineIndex(undefined)).toBeNull()
  })
})

describe('reviewTone', () => {
  it('colours a count difference', () => {
    expect(reviewTone(3)).toBe('sage')
    expect(reviewTone(-2)).toBe('danger')
    expect(reviewTone(0)).toBe('neutral')
  })
})
