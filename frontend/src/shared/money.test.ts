import { describe, expect, it } from 'vitest'
import { formatRupees, parseMoneyInput } from './money'

describe('formatRupees', () => {
  it('uses Indian grouping and two decimals', () => {
    expect(formatRupees('1090.00')).toBe('₹1,090.00')
    expect(formatRupees('750')).toBe('₹750.00')
    expect(formatRupees('0.00')).toBe('₹0.00')
    expect(formatRupees('123456.5')).toBe('₹1,23,456.50')
    expect(formatRupees('9999999.99')).toBe('₹99,99,999.99')
  })

  it('shows a dash for no amount', () => {
    expect(formatRupees(null)).toBe('—')
  })
})

describe('parseMoneyInput', () => {
  it('accepts what people type', () => {
    expect(parseMoneyInput('1,090')).toBe('1090.00')
    expect(parseMoneyInput('750.5')).toBe('750.50')
    expect(parseMoneyInput(' 735 ')).toBe('735.00')
    expect(parseMoneyInput('0')).toBe('0.00')
  })

  it('rejects anything else', () => {
    for (const bad of [' ', '', 'abc', '-1', '1.234', '10000000', '1e3', '.5']) {
      expect(parseMoneyInput(bad), bad).toBeNull()
    }
  })
})
