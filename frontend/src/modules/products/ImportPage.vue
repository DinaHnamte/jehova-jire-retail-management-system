<script setup lang="ts">
import { computed, ref } from 'vue'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { readCsvFile } from '@/shared/csv-file'
import Badge from '@/shared/ui/Badge.vue'
import { importProducts, type ImportReport } from './api'

const fileName = ref<string | null>(null)
const csv = ref<string | null>(null)
const report = ref<ImportReport | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const done = ref<ImportReport | null>(null)

const problems = computed(() => report.value?.rows.filter((r) => r.status === 'error').length ?? 0)
const canImport = computed(() => !!report.value && problems.value === 0 && report.value.created > 0 && !busy.value)

function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`
}

async function onFile(e: Event) {
  report.value = null
  done.value = null
  error.value = null
  const f = await readCsvFile(e.target as HTMLInputElement)
  if (!f) return
  if (!f.ok) {
    error.value = f.error
    return
  }
  fileName.value = f.name
  csv.value = f.text
  await check()
}

async function check() {
  if (csv.value === null) return
  busy.value = true
  try {
    report.value = await importProducts(csv.value, true)
  } catch (e) {
    error.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}

async function run() {
  if (csv.value === null || !canImport.value) return
  busy.value = true
  error.value = null
  try {
    done.value = await importProducts(csv.value, false)
    report.value = null
  } catch (e) {
    // Rows changed since the check: show the fresh report.
    if (e instanceof ApiError && e.code === 'VALIDATION_FAILED' && (e.details as ImportReport)?.rows) {
      report.value = e.details as ImportReport
    }
    error.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}

function reset() {
  fileName.value = null
  csv.value = null
  report.value = null
  done.value = null
  error.value = null
}

const statusTone = { create: 'sage', skip: 'neutral', error: 'danger' } as const
const statusLabel = { create: 'Add', skip: 'Skip', error: 'Problem' } as const
</script>

<template>
  <RouterLink to="/products" class="text-muted hover:text-ink">← Products</RouterLink>
  <h1 class="mt-3 text-3xl font-extrabold">Import products</h1>
  <p class="mt-1 max-w-prose text-muted">
    Add many products at once from a spreadsheet saved as CSV. You'll see what will happen before anything is saved.
  </p>

  <div v-if="done" class="card mt-6 border-sage bg-sage-tint p-5" role="status">
    <h2 class="text-xl font-bold text-sage-ink">Added {{ plural(done.created, 'product') }}</h2>
    <p class="mt-1 text-sage-ink">
      <template v-if="done.skipped">{{ plural(done.skipped, 'row') }} skipped (code already used). </template>
      <template v-if="done.new_categories.length">New categories: {{ done.new_categories.join(', ') }}.</template>
    </p>
    <div class="mt-4 flex flex-wrap gap-2">
      <RouterLink to="/products" class="btn-primary">See products</RouterLink>
      <button type="button" class="btn-secondary" @click="reset">Import another file</button>
    </div>
  </div>

  <template v-else>
    <section class="card mt-6 p-5 sm:p-6">
      <h2 class="text-xl font-bold">1. Prepare the file</h2>
      <p class="mt-1 text-muted">
        Required columns: <strong class="text-ink">code</strong>, <strong class="text-ink">name</strong>,
        <strong class="text-ink">selling_price</strong>. Optional: design_code (e.g. MP4952), mrp, cost, unit, category, brand, description, aliases
        (separate several with |).
      </p>
      <a href="/product-import-template.csv" download class="btn-secondary mt-4">Download the template</a>

      <h2 class="mt-8 text-xl font-bold">2. Choose the file</h2>
      <label class="mt-3 flex w-fit cursor-pointer flex-wrap items-center gap-3 rounded-control focus-within:outline-3 focus-within:outline-offset-2 focus-within:outline-primary">
        <span class="btn-primary">{{ fileName ? 'Choose another file' : 'Choose CSV file' }}</span>
        <span class="text-muted">{{ fileName ?? 'No file chosen' }}</span>
        <input type="file" accept=".csv,text/csv" class="sr-only" @change="onFile" />
      </label>
    </section>

    <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
    <p v-if="busy && !report" class="mt-4 text-muted">Checking the file…</p>

    <section v-if="report" class="mt-6" aria-labelledby="preview-h">
      <div class="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 id="preview-h" class="text-xl font-bold">3. Check and import</h2>
          <p class="mt-1 text-muted">
            {{ plural(report.created, 'product') }} to add<template v-if="report.skipped">, {{ report.skipped }} skipped</template><template v-if="problems">,
              <strong class="text-danger">{{ plural(problems, 'problem') }}</strong></template>.
            <template v-if="report.new_categories.length"> New categories: {{ report.new_categories.join(', ') }}.</template>
          </p>
        </div>
        <button v-if="report.created > 0" type="button" class="btn-primary" :disabled="!canImport" @click="run">
          {{ busy ? 'Importing…' : `Import ${plural(report.created, 'product')}` }}
        </button>
        <p v-else-if="!problems" class="font-semibold">Nothing new to add.</p>
      </div>
      <p v-if="problems" class="mt-3 rounded-control bg-golden-tint px-4 py-3 text-golden-ink">
        Fix the rows marked Problem in your spreadsheet, save it, and choose the file again. Nothing is imported until
        every row is fine.
      </p>

      <div class="card mt-4 overflow-hidden">
        <table class="w-full text-left">
          <thead class="border-b border-line text-sm text-muted">
            <tr>
              <th scope="col" class="px-4 py-3 font-semibold">Row</th>
              <th scope="col" class="px-4 py-3 font-semibold">Product</th>
              <th scope="col" class="px-4 py-3 text-right font-semibold">Result</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in report.rows" :key="r.row" class="border-b border-line align-top last:border-0">
              <td class="px-4 py-3 text-muted tabular-nums">{{ r.row }}</td>
              <td class="px-4 py-3">
                <p class="font-semibold">{{ r.name || '(no name)' }}</p>
                <p class="text-sm text-muted">{{ r.code ?? '' }}<template v-if="r.design_code"> · {{ r.design_code }}</template></p>
                <p v-if="r.note" class="mt-1 text-sm text-muted">{{ r.note }}</p>
                <ul v-if="r.errors.length" class="mt-1 space-y-0.5 text-sm text-danger">
                  <li v-for="(e, i) in r.errors" :key="i">{{ e.message }}</li>
                </ul>
              </td>
              <td class="px-4 py-3 text-right">
                <Badge :tone="statusTone[r.status]" dot>{{ statusLabel[r.status] }}</Badge>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </template>
</template>
