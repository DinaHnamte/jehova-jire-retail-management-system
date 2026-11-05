// Scoring for the OCR test page: which rotation read best, and was it right.
import type { LabelFields } from '@/modules/scanner/parse-label'
import { parseMoneyInput } from '@/shared/money'
import type { OcrLine } from './engines'

export interface Reading {
  fields: LabelFields
  lines: OcrLine[]
}

/** Higher is better: a model code matters most, then design and MRP, then confidence. */
export function score(r: Reading): number {
  const mean = r.lines.length ? r.lines.reduce((s, l) => s + l.confidence, 0) / r.lines.length : 0
  return (r.fields.code ? 4 : 0) + (r.fields.designCode ? 1 : 0) + (r.fields.mrp ? 1 : 0) + mean
}

export interface Expected {
  code: string
  design_code: string | null
  mrp: string | null
}

/** Field by field, did the reading match what's printed on the label? */
export function compare(f: LabelFields, e: Expected) {
  return {
    code: f.code?.normalized === e.code,
    design: (f.designCode ?? null) === (e.design_code ?? null),
    mrp: (f.mrp ?? null) === (e.mrp === null ? null : parseMoneyInput(e.mrp)),
  }
}
