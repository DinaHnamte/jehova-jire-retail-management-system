import { describe, expect, it, vi } from 'vitest'
import { readCsvFile } from './csv-file'

function pickerWith(file: File | null) {
  const input = document.createElement('input')
  input.type = 'file'
  Object.defineProperty(input, 'files', { value: file ? [file] : [] })
  const setValue = vi.fn()
  Object.defineProperty(input, 'value', { get: () => 'C:\\fakepath\\x.csv', set: setValue })
  return { input, setValue }
}

describe('readCsvFile', () => {
  it('reads UTF-8 text and clears the picker so the same file can be chosen again', async () => {
    const { input, setValue } = pickerWith(new File(['code,name\nB 1-1,Plate ✓\n'], 'p.csv'))
    const r = await readCsvFile(input)
    expect(r).toEqual({ ok: true, name: 'p.csv', text: 'code,name\nB 1-1,Plate ✓\n' })
    expect(setValue).toHaveBeenCalledWith('')
  })

  it('rejects a file that is not UTF-8 instead of garbling it', async () => {
    // "é" in Windows-1252 is the single byte 0xE9, which is invalid UTF-8.
    const bytes = new Uint8Array([0x63, 0x6f, 0x64, 0x65, 0x0a, 0x43, 0x61, 0x66, 0xe9])
    const { input, setValue } = pickerWith(new File([bytes], 'w.csv'))
    const r = await readCsvFile(input)
    expect(r).toMatchObject({ ok: false })
    expect(r && !r.ok ? r.error : '').toContain('CSV UTF-8')
    expect(setValue).toHaveBeenCalledWith('')
  })

  it('rejects files over 1 MB', async () => {
    const { input } = pickerWith(new File([new Uint8Array(1024 * 1024 + 1)], 'big.csv'))
    expect(await readCsvFile(input)).toMatchObject({ ok: false })
  })

  it('returns null when nothing was chosen', async () => {
    expect(await readCsvFile(pickerWith(null).input)).toBeNull()
  })
})
