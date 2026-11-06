import { defineStore } from 'pinia'
import type { SearchHit } from '@/modules/products/api'
import type { CheckoutBody, CheckoutPayment, Customer } from './api'
import { parseMoneyInput } from '@/shared/money'
import { nextKey, type IssuedKey } from '@/shared/idempotency'
import { cartProblems, cartTotals, type CartLine } from './cart'
import { deliveryBody, emptyDraft, totalWithFee, type DeliveryDraft } from './delivery'

const STORAGE_KEY = 'jj.cart.v1'

interface NewCustomer {
  phone: string
  name: string
}

interface Persisted {
  lines: CartLine[]
  orderDiscount: string
  customer: Customer | null
  newCustomer: NewCustomer | null
  key: IssuedKey | null
  fulfilment: 'counter' | 'delivery'
  delivery: DeliveryDraft
}

const MAX_QTY = 1_000_000
const MONEY = /^\d+\.\d{2}$/

function validLine(l: unknown): l is CartLine {
  if (!l || typeof l !== 'object') return false
  const x = l as Record<string, unknown>
  return (
    typeof x.productId === 'string' &&
    typeof x.name === 'string' &&
    (x.code === null || typeof x.code === 'string') &&
    typeof x.listPrice === 'string' && MONEY.test(x.listPrice) &&
    typeof x.unitPrice === 'string' && MONEY.test(x.unitPrice) &&
    typeof x.lineDiscount === 'string' && MONEY.test(x.lineDiscount) &&
    Number.isInteger(x.quantity) && (x.quantity as number) >= 1 && (x.quantity as number) <= MAX_QTY
  )
}

/** A stored cart with any malformed line is dropped whole, so `totals` can never throw. */
function load(): Partial<Persisted> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const d = JSON.parse(raw) as Partial<Persisted> | null
    if (!d || typeof d !== 'object' || !Array.isArray(d.lines) || !d.lines.every(validLine)) return {}
    if (d.orderDiscount !== undefined && !(typeof d.orderDiscount === 'string' && MONEY.test(d.orderDiscount))) return {}
    return d
  } catch {
    return {}
  }
}

/** Only a draft whose every field has the right type is kept; otherwise it starts empty. */
function loadDraft(d: unknown): DeliveryDraft {
  const base = emptyDraft()
  if (!d || typeof d !== 'object') return base
  const x = d as Record<string, unknown>
  for (const k of Object.keys(base) as (keyof DeliveryDraft)[]) {
    if (typeof x[k] !== 'string') return base
  }
  const out = { ...base, ...(x as unknown as DeliveryDraft) }
  if (out.collector !== 'driver' && out.collector !== 'shop') return base
  if (out.payMethod !== 'retain_from_collection' && out.payMethod !== 'paid_by_shop') return base
  return out
}

function save(s: Persisted) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s))
  } catch {
    // Storage full or blocked: the cart still works, it just won't survive a reload.
  }
}

function buildBody(s: Persisted, payments: CheckoutPayment[]): CheckoutBody {
  const body: CheckoutBody = {
    lines: s.lines.map((l) => ({
      product_id: l.productId,
      quantity: l.quantity,
      unit_price: l.unitPrice,
      line_discount: l.lineDiscount,
    })),
    order_discount: s.orderDiscount,
    payments: payments.map((p) => ({ method: p.method, amount: p.amount })),
  }
  if (s.customer) body.customer_id = s.customer.id
  else if (s.newCustomer) {
    const name = s.newCustomer.name.trim()
    body.new_customer = name ? { phone: s.newCustomer.phone, name } : { phone: s.newCustomer.phone }
  }
  if (s.fulfilment === 'delivery') body.delivery = deliveryBody(s.delivery)
  return body
}

const validPayment = (p: unknown): p is CheckoutPayment => {
  const x = p as Record<string, unknown> | null
  return !!x && (x.method === 'cash' || x.method === 'upi') && typeof x.amount === 'string' && MONEY.test(x.amount)
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
      fulfilment: d.fulfilment === 'delivery' ? 'delivery' : 'counter',
      delivery: loadDraft(d.delivery),
    }
  },
  getters: {
    /** Goods only, before any delivery fee. */
    goods: (s) => cartTotals(s.lines, s.orderDiscount),
    /** What the customer owes: goods, plus the delivery fee in delivery mode. */
    totals(): ReturnType<typeof cartTotals> {
      const t = this.goods
      return this.fulfilment === 'delivery' ? { ...t, total: totalWithFee(t.total, this.delivery.fee) } : t
    },
    /** Discounts the server would reject; Charge stays disabled while this is non-empty. */
    problems: (s) => cartProblems(s.lines, s.orderDiscount),
    valid(): boolean {
      return this.lines.length > 0 && this.problems.length === 0
    },
    count: (s) => s.lines.reduce((n, l) => n + l.quantity, 0),
    /**
     * The payments of a charge that was sent but not known to have gone through (its key is
     * still held and the cart has not changed since). Charging again with exactly these
     * payments resends the same body with the same key, so the server makes one sale.
     * Null when no such charge is pending.
     */
    pendingPayments: (s): CheckoutPayment[] | null => {
      if (!s.key) return null
      let sent: unknown
      try {
        sent = JSON.parse(s.key.body)
      } catch {
        return null
      }
      const payments = (sent as { payments?: unknown } | null)?.payments
      if (!Array.isArray(payments) || !payments.every(validPayment)) return null
      const ps = payments.map((p) => ({ method: p.method, amount: p.amount }))
      return JSON.stringify(buildBody(s, ps)) === s.key.body ? ps : null
    },
  },
  actions: {
    persist() {
      save({
        lines: this.lines,
        orderDiscount: this.orderDiscount,
        customer: this.customer,
        newCustomer: this.newCustomer,
        key: this.key,
        fulfilment: this.fulfilment,
        delivery: this.delivery,
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
    // Setters ignore invalid input (the value stays as it was) and store money normalised to two
    // decimals; the screen keeps its own text while typing and commits when it parses.
    setQty(productId: string, quantity: number) {
      const l = this.lines.find((x) => x.productId === productId)
      if (l && Number.isInteger(quantity) && quantity >= 1 && quantity <= MAX_QTY) l.quantity = quantity
      this.persist()
    },
    setPrice(productId: string, unitPrice: string) {
      const l = this.lines.find((x) => x.productId === productId)
      const m = parseMoneyInput(unitPrice)
      if (l && m !== null) l.unitPrice = m
      this.persist()
    },
    setLineDiscount(productId: string, discount: string) {
      const l = this.lines.find((x) => x.productId === productId)
      const m = parseMoneyInput(discount)
      if (l && m !== null) l.lineDiscount = m
      this.persist()
    },
    setOrderDiscount(discount: string) {
      const m = parseMoneyInput(discount)
      if (m !== null) this.orderDiscount = m
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
    setFulfilment(f: 'counter' | 'delivery') {
      this.fulfilment = f
      this.persist()
    },
    setDelivery(d: DeliveryDraft) {
      this.delivery = d
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
      this.fulfilment = 'counter'
      this.delivery = emptyDraft()
      this.key = null
      this.persist()
    },
    toBody(payments: CheckoutPayment[] = []): CheckoutBody {
      return buildBody(this.$state, payments)
    },
    /**
     * Idempotency key for charging with these `payments`. The key belongs to the exact
     * serialized `toBody(payments)`: the same body (a retry after a dropped response) gets
     * the same key, and any change, including changing back to an earlier body, gets a
     * fresh random key. A key is never reissued once the body has moved on, so the server
     * never sees one key with two different bodies. `clear()` forgets it after a sale.
     */
    chargeKey(payments: CheckoutPayment[] = []): string {
      const next = nextKey(this.key, this.toBody(payments))
      if (next !== this.key) {
        this.key = next
        this.persist()
      }
      return next.value
    },
  },
})
