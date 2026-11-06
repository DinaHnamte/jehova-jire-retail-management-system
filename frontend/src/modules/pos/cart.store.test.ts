import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { SearchHit } from '@/modules/products/api'
import { useCartStore } from './cart.store'

// jsdom may not ship crypto.randomUUID; shim it here, not in production code.
let n = 0
if (!globalThis.crypto?.randomUUID) {
  Object.defineProperty(globalThis, 'crypto', {
    value: { randomUUID: () => `00000000-0000-4000-8000-${String(++n).padStart(12, '0')}` },
    configurable: true,
  })
}

const hit = (id: string, price = '450.00'): SearchHit =>
  ({ id, name: `Item ${id}`, code: `C${id}`, selling_price: price, match: 'exact' }) as SearchHit

function fresh() {
  setActivePinia(createPinia())
  return useCartStore()
}

beforeEach(() => localStorage.clear())

describe('cart store', () => {
  it('add_same_product_increments', () => {
    const c = fresh()
    c.add(hit('a'))
    c.add(hit('a'))
    c.add(hit('b'))
    expect(c.lines).toHaveLength(2)
    expect(c.lines[0]!.quantity).toBe(2)
    expect(c.lines[0]).toMatchObject({ productId: 'a', code: 'Ca', listPrice: '450.00', unitPrice: '450.00' })
  })

  it('persists_and_restores_from_localStorage', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setQty('a', 3)
    c.setOrderDiscount('20.00')
    c.setNewCustomer({ phone: '9876543210', name: 'Asha' })
    const c2 = fresh() // new pinia, same localStorage
    expect(c2.lines[0]!.quantity).toBe(3)
    expect(c2.orderDiscount).toBe('20.00')
    expect(c2.newCustomer).toEqual({ phone: '9876543210', name: 'Asha' })
  })

  it('corrupt_localStorage_starts_empty', () => {
    localStorage.setItem('jj.cart.v1', '{not json')
    expect(fresh().lines).toEqual([])
    localStorage.setItem('jj.cart.v1', '{"lines":"x"}')
    expect(fresh().lines).toEqual([])
  })

  it('key_changes_when_cart_changes', () => {
    const c = fresh()
    c.add(hit('a'))
    const k1 = c.chargeKey()
    expect(c.chargeKey()).toBe(k1)
    c.setQty('a', 2)
    const k2 = c.chargeKey()
    expect(k2).not.toBe(k1)
    c.setQty('a', 1) // back to the first body
    const k3 = c.chargeKey()
    expect(k3).not.toBe(k1)
    expect(k3).not.toBe(k2)
  })

  it('key follows payments too, and survives a reload while the body is unchanged', () => {
    const c = fresh()
    c.add(hit('a'))
    const cash = [{ method: 'cash' as const, amount: '450.00' }]
    const k = c.chargeKey(cash)
    expect(c.chargeKey(cash)).toBe(k)
    expect(c.chargeKey([{ method: 'upi', amount: '450.00' }])).not.toBe(k)
    const k2 = c.chargeKey(cash)
    expect(fresh().chargeKey(cash)).toBe(k2)
  })

  it('reopening after a failed charge gives back the same payments and the same key', () => {
    const c = fresh()
    c.add(hit('a'))
    expect(c.pendingPayments).toBeNull()
    const split = [
      { method: 'cash' as const, amount: '200.00' },
      { method: 'upi' as const, amount: '250.00' },
    ]
    const k = c.chargeKey(split) // the charge fails with no reply
    const again = fresh() // the page is reloaded
    expect(again.pendingPayments).toEqual(split)
    expect(again.chargeKey(again.pendingPayments!)).toBe(k)
  })

  it('pay later is remembered as no payments', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setNewCustomer({ phone: '9876543210', name: '' })
    c.chargeKey([])
    expect(fresh().pendingPayments).toEqual([])
  })

  it('no pending payments once the cart changes or the sale is cleared', () => {
    const c = fresh()
    c.add(hit('a'))
    c.chargeKey([{ method: 'cash', amount: '450.00' }])
    c.setQty('a', 2)
    expect(c.pendingPayments).toBeNull()
    c.chargeKey([{ method: 'cash', amount: '900.00' }])
    c.clear()
    expect(c.pendingPayments).toBeNull()
    localStorage.setItem('jj.cart.v1', JSON.stringify({ lines: [], key: { body: '{bad', value: 'k' } }))
    expect(fresh().pendingPayments).toBeNull()
  })

  it('toBody shapes the request', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setLineDiscount('a', '5.00')
    c.setCustomer({ id: 'cust1', phone: '1', name: null, note: null, created_at: '' })
    expect(c.toBody([{ method: 'cash', amount: '445.00' }])).toEqual({
      lines: [{ product_id: 'a', quantity: 1, unit_price: '450.00', line_discount: '5.00' }],
      order_discount: '0.00',
      customer_id: 'cust1',
      payments: [{ method: 'cash', amount: '445.00' }],
    })
  })

  it('clear empties the cart and forgets the key', () => {
    const c = fresh()
    c.add(hit('a'))
    const k = c.chargeKey()
    c.clear()
    expect(c.lines).toEqual([])
    c.add(hit('a'))
    expect(c.chargeKey()).not.toBe(k)
    expect(fresh().lines).toHaveLength(1)
  })
})

describe('cart store: validation and keys', () => {
  it('key changes when only the customer or new-customer name changes', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setNewCustomer({ phone: '9876543210', name: 'Asha' })
    const k1 = c.chargeKey()
    c.setNewCustomer({ phone: '9876543210', name: 'Asha B' })
    const k2 = c.chargeKey()
    expect(k2).not.toBe(k1)
    c.setCustomer({ id: 'c1', phone: '1', name: null, note: null, created_at: '' })
    const k3 = c.chargeKey()
    expect(k3).not.toBe(k2)
    c.setCustomer({ id: 'c2', phone: '2', name: null, note: null, created_at: '' })
    expect(c.chargeKey()).not.toBe(k3)
  })

  it('toBody trims or omits the new-customer name', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setNewCustomer({ phone: '9876543210', name: '  Asha  ' })
    expect(c.toBody().new_customer).toEqual({ phone: '9876543210', name: 'Asha' })
    c.setNewCustomer({ phone: '9876543210', name: '   ' })
    expect(c.toBody().new_customer).toEqual({ phone: '9876543210' })
    expect(c.toBody().customer_id).toBeUndefined()
  })

  it('problems and valid report over-discounts', () => {
    const c = fresh()
    expect(c.valid).toBe(false) // empty
    c.add(hit('a'))
    expect(c.valid).toBe(true)
    c.setLineDiscount('a', '500')
    expect(c.valid).toBe(false)
    expect(c.problems[0]!.message).toBe('Discount is more than this line. Enter ₹450.00 or less.')
    c.setLineDiscount('a', '0')
    c.setOrderDiscount('451')
    expect(c.problems.map((p) => p.target)).toEqual(['order'])
  })

  it('setters ignore invalid input', () => {
    const c = fresh()
    c.add(hit('a'))
    c.setQty('a', 0)
    c.setQty('a', 1.5)
    c.setQty('a', 1_000_001)
    c.setPrice('a', 'abc')
    c.setLineDiscount('a', '-5')
    c.setOrderDiscount('1.234')
    expect(c.lines[0]).toMatchObject({ quantity: 1, unitPrice: '450.00', lineDiscount: '0.00' })
    expect(c.orderDiscount).toBe('0.00')
    c.setPrice('a', '12.5')
    expect(c.lines[0]!.unitPrice).toBe('12.50')
    expect(() => c.totals).not.toThrow()
  })

  it('a stored cart with a malformed line is dropped', () => {
    const good = { productId: 'a', code: null, name: 'x', listPrice: '1.00', unitPrice: '1.00', quantity: 1, lineDiscount: '0.00' }
    localStorage.setItem('jj.cart.v1', JSON.stringify({ lines: [good, { ...good, unitPrice: 'abc' }] }))
    expect(fresh().lines).toEqual([])
    localStorage.setItem('jj.cart.v1', JSON.stringify({ lines: [good, { ...good, quantity: 0 }] }))
    expect(fresh().lines).toEqual([])
    localStorage.setItem('jj.cart.v1', JSON.stringify({ lines: [good] }))
    expect(fresh().lines).toHaveLength(1)
  })
})
