/// <reference types="vitest/config" />
import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { VitePWA } from 'vite-plugin-pwa'

// js-clipper (used by the PaddleOCR test engine) ships a Latin-1 file the bundler can't read as UTF-8.
const latin1JsClipper: Plugin = {
  name: 'latin1-js-clipper',
  enforce: 'pre',
  load(id) {
    if (id.split('?')[0]!.endsWith('/js-clipper/clipper.js')) return readFileSync(id.split('?')[0]!, 'latin1')
  },
}

// Same policy as deploy/Caddyfile, so `vite preview` catches anything production would block.
const CSP =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' blob: data:; worker-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"

export default defineConfig({
  plugins: [
    latin1JsClipper,
    vue(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['icon.svg'],
      manifest: {
        name: 'Jehovah Jireh',
        short_name: 'JJ Retail',
        description: 'Jehovah Jireh retail management',
        start_url: '/',
        display: 'standalone',
        background_color: '#f6e9dd',
        theme_color: '#292b27',
        icons: [{ src: 'icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' }],
      },
      workbox: {
        navigateFallback: '/index.html',
        // API and health responses are never cached: the app is online-only (ADR-0002).
        navigateFallbackDenylist: [/^\/api\//, /^\/health\//],
        runtimeCaching: [],
        // OCR engines, models and test photos are fetched on demand, never precached.
        globIgnores: ['ocr/**', 'lab/**', 'assets/ocr-*', 'assets/*.wasm'],
      },
    }),
  ],
  resolve: {
    alias: [
      { find: '@', replacement: fileURLToPath(new URL('./src', import.meta.url)) },
      // CPU (WebAssembly) build only: the default entry also bundles a 30 MB WebGPU runtime.
      { find: /^onnxruntime-web$/, replacement: 'onnxruntime-web/wasm' },
    ],
  },
  build: {
    rolldownOptions: {
      output: {
        // Name the OCR test bench's heavy dependencies so the service worker skips them.
        manualChunks(id: string) {
          // One chunk per engine: OpenCV (Paddle's dependency) runs string-eval at load, so it
          // must not share a chunk with Tesseract, which works under the strict CSP.
          if (/node_modules\/.*(opencv-js|onnxruntime|@gutenye|js-clipper)/.test(id)) return 'ocr-paddle'
          if (/node_modules\/.*tesseract/.test(id)) return 'ocr-tesseract'
        },
      },
    },
  },
  preview: { headers: { 'Content-Security-Policy': CSP } },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8000',
      '/health': 'http://127.0.0.1:8000',
    },
  },
  test: { environment: 'jsdom' },
})
