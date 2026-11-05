import { describe, expect, it } from 'vitest'
import { parseLabel } from './parse-label'

// Lines as an OCR engine returns them for the photos in fixtures/labels.
const PRICE_LABEL = [
  '1 Pcs Set 4 Section Plate',
  'IP 1060-11" MP4952',
  'MRP : ₹ 750/-',
  '(Inclusive of All Taxes)',
  'Mfg. Date : Dec. 2023',
  'Marketed By: Srithai Superware India Ltd.',
  'Plot SM-22, GIDC Sanand-II(Bol), Ta.',
  'Sanand, Ahmedabad-382170. Gujarat.',
  'Customer Care Exe. No. : 9870392754',
]

describe('parseLabel', () => {
  it('reads model code, design code and MRP from a price label', () => {
    const r = parseLabel(PRICE_LABEL)
    expect(r.code?.normalized).toBe('IP1060-11')
    expect(r.designCode).toBe('MP4952')
    expect(r.mrp).toBe('750.00')
    expect(r.name).toBe('4 Section Plate')
  })

  it('reads a plain label where the design code is on its own line', () => {
    const r = parseLabel(['1 Pcs Set', '4 Section', 'Plate', 'IP 1060-11"', 'MP4952'])
    expect(r.code?.normalized).toBe('IP1060-11')
    expect(r.designCode).toBe('MP4952')
    expect(r.mrp).toBeNull()
  })

  it('finds a code in brackets after a sub-brand, with no design', () => {
    const r = parseLabel(['1 Pcs./Set Storage Box 8"(H;5)', '(W/Lid)', 'Beuniq (B 6143-8)', 'MRP: ₹ 1090/-'])
    expect(r.code?.normalized).toBe('B6143-8')
    expect(r.designCode).toBeNull()
    expect(r.mrp).toBe('1090.00')
  })

  it('handles a space before the hyphen and a design name', () => {
    const r = parseLabel(['1 Pcs Set Easy Lunch 7.8"', 'W/Plasstic Lid', 'IB 1290 -7.8" Retro Daisy', 'MRP :₹ 735/-'])
    expect(r.code?.normalized).toBe('IB1290-7.8')
    expect(r.designCode).toBeNull()
    expect(r.mrp).toBe('735.00')
  })

  it('repairs common misreadings inside the digits', () => {
    // O for 0, l for 1, S for 5
    const r = parseLabel(['IP lO6O-ll" MP49S2', 'MRP : ₹ 7SO/-'])
    expect(r.code?.normalized).toBe('IP1060-11')
    expect(r.designCode).toBe('MP4952')
    expect(r.mrp).toBe('750.00')
  })

  it('reads the MRP when the rupee sign is misread', () => {
    expect(parseLabel(['MRP : Z 750/-']).mrp).toBe('750.00')
    expect(parseLabel(['MRP: Rs.1,090/-']).mrp).toBe('1090.00')
  })

  it('never takes the phone number, pin code or plot number for a code', () => {
    const r = parseLabel(PRICE_LABEL.slice(5))
    expect(r.code).toBeNull()
    expect(r.designCode).toBeNull()
    expect(r.mrp).toBeNull()
  })

  it('returns empty fields for nothing', () => {
    expect(parseLabel([])).toEqual({ code: null, designCode: null, mrp: null, name: null })
  })
})
