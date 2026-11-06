import { describe, expect, it } from 'vitest'
import { buildDriverPatch, type Driver } from './drivers-api'

const saved: Driver = {
  id: 'd1', display_name: 'Lal', username: 'lal', phone: '9862000000',
  default_pay: '50.00', note: null, is_active: true, open_jobs: 0,
}
const same = { display_name: 'Lal', phone: '9862000000', default_pay: '50.00', note: '' }

describe('buildDriverPatch', () => {
  it('is empty when nothing changed', () => expect(buildDriverPatch(saved, same)).toEqual({}))
  it('sends only changed fields', () =>
    expect(buildDriverPatch(saved, { ...same, default_pay: '60.00', phone: ' 9862000001 ' })).toEqual({
      default_pay: '60.00',
      phone: '9862000001',
    }))
  it('clears a note with null', () =>
    expect(buildDriverPatch({ ...saved, note: 'x' }, { ...same, note: '  ' })).toEqual({ note: null }))
})
