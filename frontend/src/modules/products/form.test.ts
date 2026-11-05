import { describe, expect, it } from 'vitest'
import { emptyForm, toRequestBody, validateForm, type ProductForm } from './form'

const filled = (over: Partial<ProductForm> = {}): ProductForm => ({
  ...emptyForm(),
  name: '4 Section Plate',
  code: 'IP 1060-11"',
  sellingPrice: '700',
  mrp: '750',
  ...over,
})

describe('validateForm', () => {
  it('requires name, code and selling price', () => {
    const errs = validateForm(emptyForm())
    expect(Object.keys(errs).sort()).toEqual(['code', 'name', 'sellingPrice'])
    expect(validateForm(filled())).toEqual({})
  })

  it('rejects a code with no digits', () => {
    expect(validateForm(filled({ code: 'Plate' })).code).toBeTruthy()
  })

  it('rejects price above MRP with the MRP in the message', () => {
    const errs = validateForm(filled({ sellingPrice: '1100', mrp: '1,090' }))
    expect(errs.sellingPrice).toBe("Selling price can't be more than the MRP (₹1,090.00).")
  })

  it('accepts price equal to MRP', () => {
    expect(validateForm(filled({ sellingPrice: '750', mrp: '750' }))).toEqual({})
  })

  it('rejects malformed amounts', () => {
    expect(validateForm(filled({ mrp: '7.505' })).mrp).toBeTruthy()
    expect(validateForm(filled({ defaultCost: 'abc' })).defaultCost).toBeTruthy()
  })
})

describe('toRequestBody', () => {
  it('normalises money and drops empty optionals when creating', () => {
    const body = toRequestBody(filled({ sellingPrice: '1,090', mrp: '', description: '  ' }), 'create')
    expect(body.selling_price).toBe('1090.00')
    expect('mrp' in body).toBe(false)
    expect('description' in body).toBe(false)
    expect(body.code).toBe('IP 1060-11"')
    expect(body.unit).toBe('piece')
    expect(body.pack_qty).toBe(1)
  })

  it('sends null for cleared optionals when editing', () => {
    const body = toRequestBody(filled({ mrp: '', categoryId: '' }), 'edit')
    expect(body.mrp).toBeNull()
    expect(body.category_id).toBeNull()
  })

  it('leaves cost out unless the person can see it', () => {
    expect('default_cost' in toRequestBody(filled({ defaultCost: '400' }), 'edit', { includeCost: false })).toBe(false)
    expect(toRequestBody(filled({ defaultCost: '400' }), 'edit').default_cost).toBe('400.00')
  })
})

describe('design code', () => {
  it('rejects anything but letters and digits', () => {
    expect(validateForm(filled({ designCode: 'MP#4952' })).designCode).toBeTruthy()
    expect(validateForm(filled({ designCode: 'mp 4952' }))).toEqual({})
  })

  it('is sent normalised, and as null when cleared while editing', () => {
    expect(toRequestBody(filled({ designCode: 'mp 4952' }), 'create').design_code).toBe('MP4952')
    expect('design_code' in toRequestBody(filled({ designCode: '' }), 'create')).toBe(false)
    expect(toRequestBody(filled({ designCode: '' }), 'edit').design_code).toBeNull()
  })
})
