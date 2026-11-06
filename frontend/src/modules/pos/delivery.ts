// The delivery part of a sale: the draft the Charge sheet edits and how it becomes a request.
// Pure functions, no Vue, so they are easy to test.
import { formatRupees, fromPaise, parseMoneyInput, toPaise } from '@/shared/money'
import type { CheckoutBody } from './api'

export interface DeliveryDraft {
  recipientName: string
  recipientPhone: string
  addressLine: string
  locality: string
  landmark: string
  instructions: string
  fee: string
  driverPay: string
  collector: 'driver' | 'shop'
  payMethod: 'retain_from_collection' | 'paid_by_shop'
}

export interface LastAddress {
  recipient_name: string | null
  recipient_phone: string | null
  address_line: string | null
  locality: string | null
  landmark: string | null
  instructions: string | null
}

export const emptyDraft = (): DeliveryDraft => ({
  recipientName: '',
  recipientPhone: '',
  addressLine: '',
  locality: '',
  landmark: '',
  instructions: '',
  fee: '',
  driverPay: '',
  collector: 'driver',
  payMethod: 'retain_from_collection',
})

const blank = (s: string) => s.trim() === ''

/** The draft fields a customer's last delivery (or the customer) fills in. */
export type AddressFields = Pick<DeliveryDraft, 'recipientName' | 'recipientPhone' | 'addressLine' | 'locality' | 'landmark' | 'instructions'>
const addressKeys: (keyof AddressFields)[] = ['recipientName', 'recipientPhone', 'addressLine', 'locality', 'landmark', 'instructions']

/** What a prefill puts in each address field ('' where it has nothing). */
export function prefillValues(addr: LastAddress | null, customer: { name: string | null; phone: string } | null): AddressFields {
  const v = (s: string | null | undefined) => (s ?? '').trim()
  return {
    recipientName: v(addr?.recipient_name ?? customer?.name),
    recipientPhone: v(addr?.recipient_phone ?? customer?.phone),
    addressLine: v(addr?.address_line),
    locality: v(addr?.locality),
    landmark: v(addr?.landmark),
    instructions: v(addr?.instructions),
  }
}

/** A stored prefill snapshot, or null when it is not one. */
export function loadPrefill(x: unknown): AddressFields | null {
  if (!x || typeof x !== 'object') return null
  const o = x as Record<string, unknown>
  if (!addressKeys.every((k) => typeof o[k] === 'string')) return null
  return Object.fromEntries(addressKeys.map((k) => [k, o[k]])) as unknown as AddressFields
}

/**
 * Fills the address fields from the customer's last delivery (or, with none, from the customer
 * itself). A field is filled when it is empty, or when it still holds what the `previous` prefill
 * put there (another customer's address the owner didn't touch); such a field is cleared when the
 * new customer has nothing for it. Anything the owner typed is kept.
 */
export function draftFromLastAddress(
  addr: LastAddress | null,
  customer: { name: string | null; phone: string } | null,
  current: DeliveryDraft,
  previous: AddressFields | null = null,
): DeliveryDraft {
  const d = { ...current }
  const next = prefillValues(addr, customer)
  for (const k of addressKeys) {
    const untouched = previous !== null && current[k].trim() === previous[k]
    if (blank(current[k]) || untouched) d[k] = next[k]
  }
  return d
}

const text = (s: string) => (blank(s) ? null : s.trim())

export function deliveryBody(d: DeliveryDraft): NonNullable<CheckoutBody['delivery']> {
  const body = {
    recipient_name: d.recipientName.trim(),
    recipient_phone: d.recipientPhone.trim(),
    address_line: d.addressLine.trim(),
    locality: d.locality.trim(),
    landmark: text(d.landmark),
    instructions: text(d.instructions),
    fee: parseMoneyInput(d.fee) ?? d.fee.trim(),
    collector: d.collector,
    pay_method: d.collector === 'shop' ? 'paid_by_shop' : d.payMethod,
  } as NonNullable<CheckoutBody['delivery']> & { driver_pay?: string }
  if (!blank(d.driverPay)) body.driver_pay = parseMoneyInput(d.driverPay) ?? d.driverPay.trim()
  return body
}

/** Client-side required fields, keyed like the server's `delivery.<field>` errors. */
export function draftErrors(d: DeliveryDraft): Record<string, string> {
  const e: Record<string, string> = {}
  if (blank(d.recipientName)) e['delivery.recipient_name'] = 'Enter who the delivery is for.'
  if (blank(d.recipientPhone)) e['delivery.recipient_phone'] = "Enter the recipient's phone number."
  if (blank(d.addressLine)) e['delivery.address_line'] = 'Enter the address.'
  if (blank(d.locality)) e['delivery.locality'] = 'Enter the locality.'
  if (blank(d.fee)) e['delivery.fee'] = 'Enter the delivery fee, or 0 for free.'
  else if (parseMoneyInput(d.fee) === null) e['delivery.fee'] = 'Enter an amount like 50 or 49.50.'
  if (!blank(d.driverPay) && parseMoneyInput(d.driverPay) === null) e['delivery.driver_pay'] = 'Enter an amount like 30 or 29.50.'
  return e
}

/** The line under the amount fields: who collects what is owing. */
export function collectLine(collector: 'driver' | 'shop', owingPaise: bigint): string {
  if (owingPaise <= 0n) return 'Paid in full'
  return `${collector === 'shop' ? 'You collect' : 'Driver collects'} ${formatRupees(fromPaise(owingPaise))}`
}

/** Goods total plus the fee, in paise; a blank or bad fee counts as zero. */
export function totalWithFee(goods: string, fee: string): string {
  const f = parseMoneyInput(fee)
  return fromPaise(toPaise(goods) + (f ? toPaise(f) : 0n))
}
