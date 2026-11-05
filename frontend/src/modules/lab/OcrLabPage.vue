<script setup lang="ts">
// OCR test bench (Architecture.md §7.5): compares engines on the label corpus and on a
// live photo, on the phone it runs on. Admin-only; results are copied out by hand.
import { computed, reactive, ref, shallowRef } from 'vue'
import { parseLabel, type LabelFields } from '@/modules/scanner/parse-label'
import { loadPaddle, loadTesseract, prepare, type Engine, type OcrLine } from './engines'
import { compare, score, type Expected } from './score'

type EngineId = Engine['id']
const LOADERS: Record<EngineId, () => Promise<Engine>> = { tesseract: loadTesseract, paddle: loadPaddle }
const LABELS = [
  'ip1060-11-mp4952-plain',
  'ip1060-11-mp4952-price',
  'ip1060-11-mg4705-price-blurry',
  'b6143-8-beuniq-price',
  'ib1290-7.8-retro-daisy-price',
]

const settings = reactive({ maxEdge: 1600, tryTurns: true, engines: { tesseract: true, paddle: true } })
const engines = shallowRef<Partial<Record<EngineId, Engine>>>({})
const loadInfo = reactive<Partial<Record<EngineId, { ms: number; mb: number }>>>({})
const busy = ref<string | null>(null)
const error = ref<string | null>(null)

interface Row {
  engine: EngineId
  image: string
  ms: number
  turn: number
  fields: LabelFields
  lines: OcrLine[]
  ok?: { code: boolean; design: boolean; mrp: boolean }
}
const rows = ref<Row[]>([])
const photo = ref<Row[]>([])

const device = {
  ua: navigator.userAgent,
  cores: navigator.hardwareConcurrency ?? null,
  memoryGb: (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? null,
  screen: `${screen.width}×${screen.height} @${devicePixelRatio}x`,
}

function downloadedMb(since: number): number {
  const bytes = performance
    .getEntriesByType('resource')
    .filter((e) => e.startTime >= since)
    .reduce((s, e) => s + ((e as PerformanceResourceTiming).transferSize || 0), 0)
  return Math.round((bytes / 1e6) * 10) / 10
}

async function getEngine(id: EngineId): Promise<Engine> {
  const have = engines.value[id]
  if (have) return have
  busy.value = `Loading ${id}…`
  const t0 = performance.now()
  const e = await LOADERS[id]()
  loadInfo[id] = { ms: Math.round(performance.now() - t0), mb: downloadedMb(t0) }
  engines.value = { ...engines.value, [id]: e }
  return e
}

async function loadImage(src: string | Blob): Promise<ImageBitmap> {
  const blob = typeof src === 'string' ? await (await fetch(src)).blob() : src
  return createImageBitmap(blob)
}

/** Run one engine on one image, trying rotations; keep the best reading. */
async function read(engine: Engine, img: ImageBitmap): Promise<Omit<Row, 'engine' | 'image'>> {
  const turns = (settings.tryTurns ? [0, 90, 270] : [0]) as (0 | 90 | 270)[]
  const t0 = performance.now()
  let best: { turn: number; fields: LabelFields; lines: OcrLine[] } | null = null
  for (const turn of turns) {
    const lines = await engine.recognize(prepare(img, settings.maxEdge, turn))
    const fields = parseLabel(lines.map((l) => l.text))
    const r = { turn, fields, lines }
    if (!best || score(r) > score(best)) best = r
    // Upright already found everything: no need to try sideways.
    if (fields.code && fields.mrp && turn === 0) break
  }
  return { ms: Math.round(performance.now() - t0), ...best! }
}

const selected = computed(() => (Object.keys(settings.engines) as EngineId[]).filter((k) => settings.engines[k]))

async function runCorpus() {
  error.value = null
  rows.value = []
  try {
    for (const id of selected.value) {
      const engine = await getEngine(id)
      for (const name of LABELS) {
        busy.value = `${engine.label}: ${name}`
        const [img, expected] = await Promise.all([
          loadImage(`/lab/labels/${name}.jpg`),
          fetch(`/lab/labels/${name}.json`).then((r) => r.json() as Promise<Expected>),
        ])
        const r = await read(engine, img)
        rows.value = [...rows.value, { engine: id, image: name, ...r, ok: compare(r.fields, expected) }]
      }
    }
  } catch (e) {
    error.value = describe(e)
  } finally {
    busy.value = null
  }
}

async function onPhoto(ev: Event) {
  const input = ev.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  error.value = null
  photo.value = []
  try {
    const img = await loadImage(file)
    for (const id of selected.value) {
      const engine = await getEngine(id)
      busy.value = `${engine.label}: your photo`
      photo.value = [...photo.value, { engine: id, image: file.name, ...(await read(engine, img)) }]
    }
  } catch (e) {
    error.value = describe(e)
  } finally {
    busy.value = null
  }
}

const summary = computed(() =>
  selected.value.map((id) => {
    const rs = rows.value.filter((r) => r.engine === id)
    const times = rs.map((r) => r.ms).sort((a, b) => a - b)
    const count = (k: 'code' | 'design' | 'mrp') => rs.filter((r) => r.ok?.[k]).length
    return {
      id,
      load: loadInfo[id],
      median: times[Math.floor(times.length / 2)] ?? null,
      max: times.at(-1) ?? null,
      code: count('code'),
      design: count('design'),
      mrp: count('mrp'),
      n: rs.length,
    }
  }),
)

const report = computed(() =>
  JSON.stringify(
    {
      device,
      settings: { maxEdge: settings.maxEdge, tryTurns: settings.tryTurns },
      summary: summary.value,
      rows: rows.value.map(({ lines, fields, ...r }) => ({ ...r, code: fields.code?.normalized ?? null, design: fields.designCode, mrp: fields.mrp, lines: lines.length })),
      photo: photo.value.map((r) => ({ engine: r.engine, ms: r.ms, turn: r.turn, code: r.fields.code?.normalized ?? null, design: r.fields.designCode, mrp: r.fields.mrp, text: r.lines.map((l) => l.text) })),
    },
    null,
    1,
  ),
)
const copied = ref(false)
async function copyReport() {
  try {
    if (navigator.share) await navigator.share({ title: 'OCR test results', text: report.value })
    else await navigator.clipboard.writeText(report.value)
    copied.value = true
  } catch {
    // Share sheet dismissed; the report is still shown below for copying by hand.
  }
}

/** Error text with the top of the stack, so a failure on a phone can be diagnosed. */
function describe(e: unknown): string {
  if (!(e instanceof Error)) return String(e)
  if (e.name === 'EvalError')
    return 'PaddleOCR is blocked by the page’s security settings. Open this page directly — type or reload jehovah-jire.hnamtek.in/lab/ocr — rather than navigating to it, or untick PaddleOCR.'
  const stack = (e.stack ?? '').split('\n').slice(0, 6).join('\n')
  return `${e.name}: ${e.message}\n${stack}`
}

const tick = (b?: boolean) => (b ? '✓' : '✗')
</script>

<template>
  <h1 class="text-3xl font-extrabold">OCR test</h1>
  <p class="mt-1 max-w-prose text-muted">
    Compares two ways of reading box labels on this phone. Run it on Wi-Fi the first time: the engines download about
    30–60 MB once.
  </p>

  <section class="card mt-6 space-y-4 p-5">
    <fieldset class="flex flex-wrap gap-x-6 gap-y-2">
      <legend class="mb-2 font-semibold">Engines</legend>
      <label class="flex items-center gap-2"><input v-model="settings.engines.paddle" type="checkbox" class="size-5" /> PaddleOCR</label>
      <label class="flex items-center gap-2"><input v-model="settings.engines.tesseract" type="checkbox" class="size-5" /> Tesseract</label>
    </fieldset>
    <div class="flex flex-wrap gap-x-6 gap-y-3">
      <label class="flex items-center gap-2">
        Image size
        <select v-model.number="settings.maxEdge" class="field w-auto py-1.5">
          <option :value="1200">1200 px</option>
          <option :value="1600">1600 px</option>
          <option :value="2000">2000 px</option>
        </select>
      </label>
      <label class="flex items-center gap-2"><input v-model="settings.tryTurns" type="checkbox" class="size-5" /> Also try sideways</label>
    </div>
    <div class="flex flex-wrap gap-2">
      <button type="button" class="btn-primary" :disabled="!!busy || !selected.length" @click="runCorpus">Run on the 5 sample labels</button>
      <label class="btn-secondary cursor-pointer focus-within:outline-3 focus-within:outline-primary">
        Take a photo of a label
        <input type="file" accept="image/*" capture="environment" class="sr-only" :disabled="!!busy" @change="onPhoto" />
      </label>
    </div>
    <p v-if="busy" class="font-semibold text-primary-ink" role="status">{{ busy }}</p>
    <p v-if="error" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium break-words whitespace-pre-wrap text-danger">{{ error }}</p>
  </section>

  <section v-if="summary.some((s) => s.load)" class="mt-6">
    <h2 class="text-xl font-bold">Summary</h2>
    <div class="card mt-3 overflow-x-auto">
      <table class="w-full text-left text-sm">
        <thead class="border-b border-line text-muted">
          <tr>
            <th class="px-4 py-2.5">Engine</th>
            <th class="px-4 py-2.5 text-right">First load</th>
            <th class="px-4 py-2.5 text-right">Download</th>
            <th class="px-4 py-2.5 text-right">Per label (median / max)</th>
            <th class="px-4 py-2.5 text-right">Code</th>
            <th class="px-4 py-2.5 text-right">Design</th>
            <th class="px-4 py-2.5 text-right">MRP</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in summary" :key="s.id" class="border-b border-line last:border-0">
            <td class="px-4 py-2.5 font-semibold">{{ s.id }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.load ? `${(s.load.ms / 1000).toFixed(1)} s` : '—' }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.load ? `${s.load.mb} MB` : '—' }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.median !== null ? `${(s.median / 1000).toFixed(1)} / ${((s.max ?? 0) / 1000).toFixed(1)} s` : '—' }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.n ? `${s.code}/${s.n}` : '—' }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.n ? `${s.design}/${s.n}` : '—' }}</td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ s.n ? `${s.mrp}/${s.n}` : '—' }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>

  <section v-if="rows.length" class="mt-6">
    <h2 class="text-xl font-bold">Sample labels</h2>
    <div class="card mt-3 overflow-x-auto">
      <table class="w-full text-left text-sm">
        <thead class="border-b border-line text-muted">
          <tr>
            <th class="px-4 py-2.5">Label</th>
            <th class="px-4 py-2.5">Engine</th>
            <th class="px-4 py-2.5">Read</th>
            <th class="px-4 py-2.5 text-right">Time</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(r, i) in rows" :key="i" class="border-b border-line align-top last:border-0">
            <td class="px-4 py-2.5">{{ r.image }}</td>
            <td class="px-4 py-2.5">{{ r.engine }}</td>
            <td class="px-4 py-2.5">
              <span :class="r.ok?.code ? 'text-sage-ink' : 'text-danger'">{{ tick(r.ok?.code) }} {{ r.fields.code?.normalized ?? 'no code' }}</span>
              · <span :class="r.ok?.design ? 'text-sage-ink' : 'text-danger'">{{ tick(r.ok?.design) }} {{ r.fields.designCode ?? 'no design' }}</span>
              · <span :class="r.ok?.mrp ? 'text-sage-ink' : 'text-danger'">{{ tick(r.ok?.mrp) }} {{ r.fields.mrp ?? 'no MRP' }}</span>
              <span class="text-muted"> · turned {{ r.turn }}°</span>
            </td>
            <td class="px-4 py-2.5 text-right tabular-nums">{{ (r.ms / 1000).toFixed(1) }} s</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>

  <section v-if="photo.length" class="mt-6">
    <h2 class="text-xl font-bold">Your photo</h2>
    <div v-for="(r, i) in photo" :key="i" class="card mt-3 p-4">
      <p class="font-semibold">{{ r.engine }} · {{ (r.ms / 1000).toFixed(1) }} s · turned {{ r.turn }}°</p>
      <p class="mt-1">Code {{ r.fields.code?.normalized ?? '—' }} · Design {{ r.fields.designCode ?? '—' }} · MRP {{ r.fields.mrp ?? '—' }}</p>
      <pre class="mt-2 overflow-x-auto rounded-control bg-canvas p-3 text-xs whitespace-pre-wrap">{{ r.lines.map((l) => l.text).join('\n') }}</pre>
    </div>
  </section>

  <section v-if="rows.length || photo.length" class="mt-6">
    <button type="button" class="btn-primary" @click="copyReport">{{ copied ? 'Shared' : 'Share results' }}</button>
    <details class="mt-3">
      <summary class="cursor-pointer text-muted">Show the report text</summary>
      <pre class="mt-2 overflow-x-auto rounded-control bg-surface p-3 text-xs">{{ report }}</pre>
    </details>
  </section>

  <p class="mt-6 text-sm text-muted">{{ device.ua }} · {{ device.cores }} cores · {{ device.memoryGb ?? '?' }} GB · {{ device.screen }}</p>
</template>
