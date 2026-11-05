// Copy OCR engine files, models and the label photo corpus into public/, so the app
// serves them from its own domain (the CSP allows nothing else). Runs before dev/build.
// Output folders are git-ignored; their sources are pinned in package.json / fixtures/.
import { cpSync, existsSync, mkdirSync, readdirSync, realpathSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
// A package's folder: a direct dependency of the app, or (with `from`) a dependency of
// another package, which pnpm places next to it.
const pkgDir = (name, from) => {
  const dir = from ? join(dirname(realpathSync(from)), ...name.split('/').slice(name.startsWith('@') ? 1 : 0)) : join(root, 'node_modules', name)
  const scoped = from && name.startsWith('@') ? join(dirname(dirname(realpathSync(from))), name) : dir
  if (existsSync(scoped)) return scoped
  throw new Error(`can't find ${name}`)
}

const out = join(root, 'public', 'ocr')
rmSync(out, { recursive: true, force: true })
const copy = (from, to) => {
  mkdirSync(dirname(to), { recursive: true })
  cpSync(from, to)
}

// Tesseract: worker script, wasm cores (it picks one by CPU features), English data.
const tess = pkgDir('tesseract.js')
copy(join(tess, 'dist/worker.min.js'), join(out, 'tesseract/worker.min.js'))
const core = pkgDir('tesseract.js-core', tess)
for (const f of readdirSync(core).filter((f) => /^tesseract-core.*-lstm\.(wasm\.js|js|wasm)$/.test(f))) {
  copy(join(core, f), join(out, 'tesseract/core', f))
}
copy(join(pkgDir('@tesseract.js-data/eng'), '4.0.0_best_int/eng.traineddata.gz'), join(out, 'tesseract/lang/eng.traineddata.gz'))

// ONNX Runtime's wasm is emitted by the bundler next to the app's assets (see vite.config.ts).

// PaddleOCR PP-OCRv4 models.
const browser = pkgDir('@gutenye/ocr-browser')
const models = join(pkgDir('@gutenye/ocr-models', browser), 'assets')
copy(join(models, 'ch_PP-OCRv4_det_infer.onnx'), join(out, 'paddle/det.onnx'))
copy(join(models, 'ch_PP-OCRv4_rec_infer.onnx'), join(out, 'paddle/rec.onnx'))
copy(join(models, 'ppocr_keys_v1.txt'), join(out, 'paddle/keys.txt'))

// Label photo corpus for the /lab/ocr test page.
const labels = join(root, '..', 'fixtures', 'labels')
const labOut = join(root, 'public', 'lab', 'labels')
rmSync(labOut, { recursive: true, force: true })
if (existsSync(labels)) {
  for (const f of readdirSync(labels).filter((f) => /\.(jpg|json)$/.test(f))) copy(join(labels, f), join(labOut, f))
}
console.log('ocr assets copied')
