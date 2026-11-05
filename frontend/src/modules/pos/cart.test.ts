import { describe, expect, it } from 'vitest'
import { cartProblems, cartTotals, lineNet, percentOff, roundDownTo, type CartLine } from './cart'

const line = (over: Partial<CartLine> = {}): CartLine => ({
  productId: 'p1',
  code: 'A1',
  name: 'Thing',
  listPrice: '450.00',
  unitPrice: '450.00',
  quantity: 2,
  lineDiscount: '10.00',
  ...over,
})

describe('lineNet', () => {
  it('is qty x price less the line discount', () => {
    expect(lineNet(line())).toBe('890.00')
  })
  it('is null when the discount is more than the line', () => {
    expect(lineNet(line({ lineDiscount: '900.01' }))).toBeNull()
    expect(lineNet(line({ lineDiscount: '900.00' }))).toBe('0.00')
  })
})

describe('cartTotals', () => {
  it('takes the order discount off the subtotal', () => {
    const t = cartTotals([line(), line({ quantity: 1, lineDiscount: '0' })], '50.00')
    expect(t).toEqual({ subtotal: '1340.00', discount: '50.00', total: '1290.00', overDiscount: false })
  })
  it('flags an order discount above the subtotal instead of capping it', () => {
    expect(cartTotals([line()], '9999')).toEqual({
      subtotal: '890.00', discount: '9999.00', total: '0.00', overDiscount: true,
    })
    expect(cartTotals([line()], '890').overDiscount).toBe(false)
  })
  it('empty cart is zero', () => {
    expect(cartTotals([], '0.00').total).toBe('0.00')
  })
})

describe('percentOff', () => {
  it('10% of 999.00', () => expect(percentOff('999.00', 10)).toBe('99.90'))
  it('rounds half up to the paise', () => {
    expect(percentOff('0.05', 10)).toBe('0.01') // 0.005 -> 0.01
    expect(percentOff('0.04', 10)).toBe('0.00')
    expect(percentOff('10.05', 5)).toBe('0.50') // 0.5025
  })
})

describe('roundDownTo', () => {
  it('gives the discount that reaches the target', () => {
    expect(roundDownTo('1043.50', '1000')).toBe('43.50')
  })
  it('is zero when already at target', () => expect(roundDownTo('1000.00', '1000')).toBe('0.00'))
  it('is null when the target is above the total', () => {
    expect(roundDownTo('900', '1000')).toBeNull()
  })
})

describe('cartProblems', () => {
  it('is empty for a valid cart', () => expect(cartProblems([line()], '10')).toEqual([]))
  it('names the line and the order discount, worded like the server', () => {
    const p = cartProblems([line({ lineDiscount: '1000' })], '5')
    expect(p).toEqual([
      { target: 'p1', field: 'line_discount', message: 'Discount is more than this line. Enter ₹900.00 or less.' },
      { target: 'order', field: 'order_discount', message: 'Discount is more than the bill. Enter ₹0.00 or less.' },
    ])
  })
})
