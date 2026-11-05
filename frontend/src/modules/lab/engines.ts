// OCR engines under test (Architecture.md §7.5). Each loads lazily, from our own domain.
export interface OcrLine {
  text: string
  /** 0–1. */
  confidence: number
}

export interface Engine {
  id: 'tesseract' | 'paddle'
  label: string
  recognize(canvas: HTMLCanvasElement): Promise<OcrLine[]>
}

export async function loadTesseract(): Promise<Engine> {
  const { createWorker, OEM } = await import('tesseract.js')
  const worker = await createWorker('eng', OEM.LSTM_ONLY, {
    workerPath: '/ocr/tesseract/worker.min.js',
    corePath: '/ocr/tesseract/core',
    langPath: '/ocr/tesseract/lang',
    workerBlobURL: false,
  })
  return {
    id: 'tesseract',
    label: 'Tesseract 7',
    async recognize(canvas) {
      const { data } = await worker.recognize(canvas)
      const conf = (data.confidence ?? 0) / 100
      return data.text
        .split('\n')
        .map((t) => t.trim())
        .filter(Boolean)
        .map((text) => ({ text, confidence: conf }))
    },
  }
}

export async function loadPaddle(): Promise<Engine> {
  const ort = await import('onnxruntime-web')
  // Multi-threading needs cross-origin isolation, which the site doesn't have.
  ort.env.wasm.numThreads = 1
  const { default: Ocr } = await import('@gutenye/ocr-browser')
  const ocr = await Ocr.create({
    models: {
      detectionPath: '/ocr/paddle/det.onnx',
      recognitionPath: '/ocr/paddle/rec.onnx',
      dictionaryPath: '/ocr/paddle/keys.txt',
    },
  })
  return {
    id: 'paddle',
    label: 'PaddleOCR (PP-OCRv4)',
    async recognize(canvas) {
      const ctx = canvas.getContext('2d', { willReadFrequently: true })!
      const img = ctx.getImageData(0, 0, canvas.width, canvas.height)
      const r = await ocr.detect({ data: img.data, width: img.width, height: img.height })
      return (r.texts as { text: string; mean: number }[]).map((t) => ({ text: t.text, confidence: t.mean }))
    },
  }
}

/** Downscale so the long edge is at most `maxEdge`, then rotate by 0/90/180/270. */
export function prepare(img: CanvasImageSource & { width: number; height: number }, maxEdge: number, turn: 0 | 90 | 180 | 270): HTMLCanvasElement {
  const scale = Math.min(1, maxEdge / Math.max(img.width, img.height))
  const w = Math.round(img.width * scale)
  const h = Math.round(img.height * scale)
  const c = document.createElement('canvas')
  const sideways = turn === 90 || turn === 270
  c.width = sideways ? h : w
  c.height = sideways ? w : h
  const ctx = c.getContext('2d', { willReadFrequently: true })!
  ctx.translate(c.width / 2, c.height / 2)
  ctx.rotate((turn * Math.PI) / 180)
  ctx.drawImage(img, -w / 2, -h / 2, w, h)
  return c
}
