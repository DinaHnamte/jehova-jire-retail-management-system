// Turn OCR'd label lines into product fields (Architecture.md §7.2, ADR-0003).
// Proposes values only: a person always checks them before anything is saved.
import { normalizeCode, normalizeDesign, type ProductCode } from '@/shared/code'
import { parseMoneyInput } from '@/shared/money'

export interface LabelFields {
  /** Model code, e.g. IP1060-11. */
  code: ProductCode | null
  /** Design/colour code, e.g. MP4952. */
  designCode: string | null
  /** MRP as a two-decimal string. */
  mrp: string | null
  /** Product name from the "1 Pcs Set …" line, when it's on one line. */
  name: string | null
}

// Characters OCR often reads in place of digits on these labels.
const DIGITISH = '0-9OQIL|SBZ'
const TO_DIGIT: Record<string, string> = { O: '0', Q: '0', I: '1', L: '1', '|': '1', S: '5', B: '8', Z: '2' }
const digitise = (s: string) => s.replace(/[OQIL|SBZ]/g, (c) => TO_DIGIT[c] ?? c)
const realDigits = (s: string) => (s.match(/[0-9]/g) ?? []).length

// Letters, four digits, hyphen, size: `IP 1060-11"`, `IB 1290 -7.8"`, `B 6143-8`.
const MODEL = new RegExp(
  `(?<![A-Z0-9])([A-Z]{1,3})[\\s-]?([${DIGITISH}]{4})\\s?-\\s?([${DIGITISH}]{1,3}(?:\\.[${DIGITISH}]{1,2})?)(?![0-9])`,
)
// Two letters and four digits with no size after it: `MP4952`.
const DESIGN = new RegExp(`(?<![A-Z0-9])([A-Z]{2})\\s?([${DIGITISH}]{4})(?![0-9A-Z]|\\s?-\\s?[0-9])`, 'g')
const MRP = /MRP[^0-9]{0,8}?([0-9][0-9OSIL,]*(?:\.[0-9]{1,2})?)/
const PACK_LINE = /^\s*\d+\s*PCS\.?\s*\/?\s*SET\s*(.*)$/i

function findModel(lines: string[]): { code: ProductCode; line: number; match: string } | null {
  for (const [i, line] of lines.entries()) {
    const m = MODEL.exec(line)
    if (!m) continue
    const [, prefix, digits, size] = m as unknown as [string, string, string, string]
    // The hyphen-and-size shape is a strong signal, so one real digit is enough here.
    if (realDigits(digits) < 1) continue
    const code = normalizeCode(`${prefix} ${digitise(digits)}-${digitise(size)}`)
    if (code) return { code, line: i, match: m[0] }
  }
  return null
}

function findDesign(lines: string[], model: { line: number; match: string; code: ProductCode } | null): string | null {
  // The design sits on the model code's line or on its own line, never in the address block.
  const candidates = model
    ? [lines[model.line]!.replace(model.match, ' '), ...lines.filter((_, i) => i !== model.line)]
    : lines
  for (const line of candidates) {
    for (const m of line.matchAll(DESIGN)) {
      const [, letters, digits] = m as unknown as [string, string, string]
      if (realDigits(digits) < 2) continue
      const d = normalizeDesign(`${letters}${digitise(digits)}`)
      if (d.ok && d.design && d.design !== model?.code.base) return d.design
    }
  }
  return null
}

function findMrp(lines: string[]): string | null {
  for (const line of lines) {
    const m = MRP.exec(line)
    if (m) return parseMoneyInput(digitise(m[1]!).replace(/,/g, ''))
  }
  return null
}

export function parseLabel(rawLines: string[]): LabelFields {
  const lines = rawLines.map((l) => l.normalize('NFKC').toUpperCase())
  const model = findModel(lines)
  const nameMatch = rawLines.map((l) => PACK_LINE.exec(l.trim())).find((m) => m && m[1]!.trim())
  return {
    code: model?.code ?? null,
    designCode: findDesign(lines, model),
    mrp: findMrp(lines),
    name: nameMatch ? nameMatch[1]!.trim() : null,
  }
}
