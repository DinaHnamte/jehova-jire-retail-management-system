import { defineStore } from 'pinia'
import type { SearchHit } from '@/modules/products/api'
import type { CheckoutBody, CheckoutPayment, Customer } from './api'
import { cartTotals, type CartLine } from './cart'

const STORAGE_KEY = 'jj.cart.v1'

interface NewCustomer {
  phone: string
  name: string
}

interface ChargeKey {
  /** The exact serialized body this key was issued for. */
  body: string
  value: string
}

interface Persisted {
  lines: CartLine[]
  orderDiscount: string
  customer: Customer | null
  newCustomer: NewCustomer | null
  key: ChargeKey | null
}

function load(): Partial<Persisted> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const d = JSON.parse(raw) as Partial<Persisted> | null
    if (!d || typeof d !== 'object' || !Array.isArray(d.lines)) return {}
    return d
  } catch {
    return {}
  }
}

function save(s: Persisted) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s))
  } catch {
    // Storage full or blocked: the cart still works, it just won't survive a reload.
  }
}

export const useCartStore = defineStore('cart', {
  state: (): Persisted => {
    const d = load()
    return {
      lines: d.lines ?? [],
      orderDiscount: typeof d.orderDiscount === 'string' ? d.orderDiscount : '0.00',
      customer: d.customer ?? null,
      newCustomer: d.newCustomer ?? null,
      key: d.key ?? null,
    }
  },
  getters: {
    totals: (s) => cartTotals(s.lines, s.orderDiscount),
    count: (s) => s.lines.reduce((n, l) => n + l.quantity, 0),
  },
  actions: {
    persist() {
      save({
        lines: this.lines,
        orderDiscount: this.orderDiscount,
        customer: this.customer,
        newCustomer: this.newCustomer,
        key: this.key,
      })
    },
    /** Add one of a search hit; a product already in the cart gets +1. */
    add(hit: SearchHit) {
      const existing = this.lines.find((l) => l.productId === hit.id)
      if (existing) existing.quantity += 1
      else
        this.lines.push({
          productId: hit.id,
          code: hit.code,
          name: hit.name,
          listPrice: hit.selling_price,
          unitPrice: hit.selling_price,
          quantity: 1,
          lineDiscount: '0.00',
        })
      this.persist()
    },
    setQty(productId: string, quantity: number) {
      const l = this.lines.find((x) => x.productId === productId)
      if (l) l.quantity = quantity
      this.persist()
    },
    setPrice(productId: string, unitPrice: string) {
      const l = this.lines.find((x) => x.productId === productId)
      if (l) l.unitPrice = unitPrice
      this.persist()
    },
    setLineDiscount(productId: string, discount: string) {
      const l = this.lines.find((x) => x.productId === productId)
      if (l) l.lineDiscount = discount
      this.persist()
    },
    setOrderDiscount(discount: string) {
      this.orderDiscount = discount
      this.persist()
    },
    /** An existing customer clears any new-customer draft, and the other way round. */
    setCustomer(c: Customer | null) {
      this.customer = c
      if (c) this.newCustomer = null
      this.persist()
    },
    setNewCustomer(n: NewCustomer | null) {
      this.newCustomer = n
      if (n) this.customer = null
      this.persist()
    },
    remove(productId: string) {
      this.lines = this.lines.filter((l) => l.productId !== productId)
      this.persist()
    },
    clear() {
      this.lines = []
      this.orderDiscount = '0.00'
      this.customer = null
      this.newCustomer = null
      this.key = null
      this.persist()
    },
    toBody(payments: CheckoutPayment[] = []): CheckoutBody {
      const body: CheckoutBody = {
        lines: this.lines.map((l) => ({
          product_id: l.productId,
          quantity: l.quantity,
          unit_price: l.unitPrice,
          line_discount: l.lineDiscount,
        })),
        order_discount: this.orderDiscount,
        payments: payments.map((p) => ({ method: p.method, amount: p.amount })),
      }
      if (this.customer) body.customer_id = this.customer.id
      else if (this.newCustomer) {
        const name = this.newCustomer.name.trim()
        body.new_customer = name ? { phone: this.newCustomer.phone, name } : { phone: this.newCustomer.phone }
      }
      return body
    },
    /**
     * Idempotency key for charging with these `payments`. The key belongs to the exact
     * serialized `toBody(payments)`: the same body (a retry after a dropped response) gets
     * the same key, and any change, including changing back to an earlier body, gets a
     * fresh random key. A key is never reissued once the body has moved on, so the server
     * never sees one key with two different bodies. `clear()` forgets it after a sale.
     */
    chargeKey(payments: CheckoutPayment[] = []): string {
      const body = JSON.stringify(this.toBody(payments))
      if (!this.key || this.key.body !== body) {
        this.key = { body, value: crypto.randomUUID() }
        this.persist()
      }
      return this.key.value
    },
  },
})
