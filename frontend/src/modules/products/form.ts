// Product form state and checks. Mirrors the server rules (backend catalog/domain.rs)
// so people see problems before saving; the server still decides.
import { normalizeCode } from '@/shared/code'
import { formatRupees, parseMoneyInput } from '@/shared/money'
import type { Product, ProductBody } from './api'

export const UNITS = ['piece', 'set', 'pack', 'box', 'pair', 'dozen'] as const

export interface ProductForm {
  name: string
  code: string
  categoryId: string
  unit: string
  packQty: string
  sellingPrice: string
  mrp: string
  defaultCost: string
  description: string
  hsnCode: string
}

export type FormErrors = Partial<Record<keyof ProductForm, string>>

export function emptyForm(): ProductForm {
  return {
    name: '',
    code: '',
    categoryId: '',
    unit: 'piece',
    packQty: '1',
    sellingPrice: '',
    mrp: '',
    defaultCost: '',
    description: '',
    hsnCode: '',
  }
}

export function fromProduct(p: Product): ProductForm {
  const code = p.identifiers.find((i) => i.type === 'product_code')
  return {
    name: p.name,
    code: code?.raw_value ?? p.code ?? '',
    categoryId: p.category?.id ?? '',
    unit: p.unit,
    packQty: String(p.pack_qty),
    sellingPrice: p.selling_price,
    mrp: p.mrp ?? '',
    defaultCost: p.default_cost ?? '',
    description: p.description ?? '',
    hsnCode: p.hsn_code ?? '',
  }
}

/** Server field names → form field names, for showing server errors on the right input. */
export const SERVER_FIELDS: Record<string, keyof ProductForm> = {
  name: 'name',
  code: 'code',
  category_id: 'categoryId',
  unit: 'unit',
  pack_qty: 'packQty',
  selling_price: 'sellingPrice',
  mrp: 'mrp',
  default_cost: 'defaultCost',
  description: 'description',
  hsn_code: 'hsnCode',
}

const AMOUNT_HELP = 'Enter an amount like 750 or 750.50.'

export function validateForm(f: ProductForm): FormErrors {
  const e: FormErrors = {}
  const name = f.name.trim()
  if (!name || name.length > 200) e.name = 'Enter a name of 1 to 200 characters.'
  if (!normalizeCode(f.code)) e.code = 'Enter the model code printed on the label, for example IP 1060-11.'

  const price = f.sellingPrice.trim() ? parseMoneyInput(f.sellingPrice) : null
  if (!f.sellingPrice.trim()) e.sellingPrice = 'Enter the selling price.'
  else if (price === null) e.sellingPrice = AMOUNT_HELP

  const mrp = f.mrp.trim() ? parseMoneyInput(f.mrp) : null
  if (f.mrp.trim() && mrp === null) e.mrp = AMOUNT_HELP
  if (f.defaultCost.trim() && parseMoneyInput(f.defaultCost) === null) e.defaultCost = AMOUNT_HELP

  if (price !== null && mrp !== null && Number(price) > Number(mrp)) {
    e.sellingPrice = `Selling price can't be more than the MRP (${formatRupees(mrp)}).`
  }
  const qty = Number(f.packQty)
  if (!Number.isInteger(qty) || qty < 1 || qty > 10000) e.packQty = 'Pack quantity must be 1 to 10,000.'
  return e
}

/**
 * Request body. Creating leaves out blank optional fields; editing sends `null` so a
 * cleared field is cleared on the server. Cost is only sent by people who can see it.
 */
export function toRequestBody(
  f: ProductForm,
  mode: 'create' | 'edit',
  opts: { includeCost?: boolean } = {},
): ProductBody {
  const includeCost = opts.includeCost ?? true
  const body: ProductBody = {
    name: f.name.trim(),
    code: f.code.trim(),
    unit: f.unit,
    pack_qty: Number(f.packQty),
    selling_price: parseMoneyInput(f.sellingPrice) ?? f.sellingPrice.trim(),
  }
  const optional = (key: keyof ProductBody, value: string | null) => {
    if (value) (body as Record<string, unknown>)[key] = value
    else if (mode === 'edit') (body as Record<string, unknown>)[key] = null
  }
  optional('category_id', f.categoryId || null)
  optional('mrp', f.mrp.trim() ? (parseMoneyInput(f.mrp) ?? f.mrp.trim()) : null)
  if (includeCost) optional('default_cost', f.defaultCost.trim() ? (parseMoneyInput(f.defaultCost) ?? f.defaultCost.trim()) : null)
  optional('description', f.description.trim() || null)
  optional('hsn_code', f.hsnCode.trim() || null)
  return body
}
