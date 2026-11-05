<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { ApiError, friendlyMessage } from '@/shared/api/errors'
import { normalizeCode } from '@/shared/code'
import { createCategory, createProduct, getProduct, listCategories, updateProduct, type NamedRef } from './api'
import { SERVER_FIELDS, UNITS, emptyForm, fromProduct, toRequestBody, validateForm, type FormErrors } from './form'

const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const id = computed(() => (typeof route.params.id === 'string' ? route.params.id : null))
const editing = computed(() => id.value !== null)
const canSeeCost = computed(() => session.can('products.update'))

const form = reactive(emptyForm())
const errors = ref<FormErrors>({})
const duplicateOf = ref<{ id: string; name: string } | null>(null)
const formError = ref<string | null>(null)
const loadError = ref<string | null>(null)
const loaded = ref(!editing.value)
const saving = ref(false)
const categories = ref<NamedRef[]>([])
const newCategory = ref<string | null>(null)
const categoryError = ref<string | null>(null)

const preview = computed(() => normalizeCode(form.code))

// A single piece has no pack size.
watch(
  () => form.unit,
  (u) => {
    if (u === 'piece') form.packQty = '1'
  },
)

onMounted(async () => {
  try {
    categories.value = await listCategories()
    if (id.value) {
      Object.assign(form, fromProduct(await getProduct(id.value)))
      loaded.value = true
    }
  } catch (e) {
    loadError.value = friendlyMessage(e)
  }
})

async function addCategory() {
  categoryError.value = null
  const name = (newCategory.value ?? '').trim()
  if (!name) return
  try {
    const c = await createCategory(name)
    categories.value = [...categories.value, c].sort((a, b) => a.name.localeCompare(b.name))
    form.categoryId = c.id
    newCategory.value = null
  } catch (e) {
    categoryError.value = friendlyMessage(e)
  }
}

async function save() {
  formError.value = null
  duplicateOf.value = null
  errors.value = validateForm(form)
  if (Object.keys(errors.value).length) return focusFirstError()
  saving.value = true
  try {
    const body = toRequestBody(form, editing.value ? 'edit' : 'create', { includeCost: canSeeCost.value })
    const p = id.value ? await updateProduct(id.value, body) : await createProduct(body)
    await router.push(`/products/${p.id}`)
  } catch (e) {
    if (e instanceof ApiError && e.code === 'DUPLICATE_CODE') {
      const d = e.details as { product_id?: string; name?: string }
      errors.value = { code: e.message }
      if (d.product_id && d.name) duplicateOf.value = { id: d.product_id, name: d.name }
    } else if (e instanceof ApiError && e.code === 'VALIDATION_FAILED') {
      const field = SERVER_FIELDS[(e.details as { field?: string }).field ?? '']
      if (field) errors.value = { [field]: e.message }
      else formError.value = e.message
    } else {
      formError.value = friendlyMessage(e)
    }
  } finally {
    saving.value = false
  }
  if (Object.keys(errors.value).length || formError.value) await focusFirstError()
}

// Input ids for each form field, so a problem can be brought into view.
const INPUT_IDS: Record<keyof FormErrors, string> = {
  name: 'name',
  code: 'code',
  designCode: 'design',
  categoryId: 'category',
  unit: 'unit',
  packQty: 'pack',
  sellingPrice: 'price',
  mrp: 'mrp',
  defaultCost: 'cost',
  description: 'description',
  hsnCode: 'hsn',
}

/** Move focus (and the screen) to the first field with a problem. */
async function focusFirstError() {
  await nextTick()
  const order = Object.keys(INPUT_IDS) as (keyof FormErrors)[]
  const first = order.find((k) => errors.value[k])
  const el = first ? document.getElementById(INPUT_IDS[first]) : document.querySelector('[role=alert]')
  if (el instanceof HTMLElement) {
    el.focus({ preventScroll: true })
    el.scrollIntoView({ block: 'center', behavior: 'smooth' })
  }
}

const fieldClass = (k: keyof FormErrors) => (errors.value[k] ? 'field mt-1.5 !border-danger' : 'field mt-1.5')
</script>

<template>
  <RouterLink :to="id ? `/products/${id}` : '/products'" class="text-muted hover:text-ink">
    ← {{ id ? 'Back to product' : 'Products' }}
  </RouterLink>
  <h1 class="mt-3 text-3xl font-extrabold">{{ editing ? 'Edit product' : 'Add product' }}</h1>

  <p v-if="loadError" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ loadError }}</p>
  <p v-else-if="!loaded" class="mt-6 text-muted">Loading…</p>

  <form v-else class="mt-6 max-w-3xl space-y-6" novalidate @submit.prevent="save">
    <section class="card space-y-5 p-5 sm:p-6">
      <h2 class="text-xl font-bold">From the label</h2>
      <div>
        <label for="code" class="block font-semibold">Model code</label>
        <input
          id="code"
          v-model="form.code"
          class="font-semibold"
          :class="fieldClass('code')"
          autocomplete="off"
          autocapitalize="characters"
          spellcheck="false"
          placeholder='IP 1060-11"'
          :aria-invalid="!!errors.code"
          aria-describedby="code-help"
        />
        <p id="code-help" class="mt-1.5 text-sm" :class="errors.code ? 'text-danger' : 'text-muted'">
          <template v-if="errors.code">
            {{ errors.code }}
            <RouterLink v-if="duplicateOf" :to="`/products/${duplicateOf.id}`" class="link">Open {{ duplicateOf.name }}</RouterLink>
          </template>
          <template v-else-if="preview">Saved as <strong class="text-ink">{{ preview.normalized }}</strong>.</template>
          <template v-else>As printed on the box, without the design code.</template>
        </p>
      </div>
      <div>
        <label for="design" class="block font-semibold">Design code <span class="font-normal text-muted">(optional)</span></label>
        <input
          id="design"
          v-model="form.designCode"
          :class="fieldClass('designCode')"
          class="font-semibold sm:w-1/2"
          autocomplete="off"
          autocapitalize="characters"
          spellcheck="false"
          placeholder="MP4952"
          :aria-invalid="!!errors.designCode"
          aria-describedby="design-help"
        />
        <p id="design-help" class="mt-1.5 text-sm" :class="errors.designCode ? 'text-danger' : 'text-muted'">
          {{ errors.designCode ?? 'The colour or print code after the model code. Each design is its own product with its own stock.' }}
        </p>
      </div>
      <div>
        <label for="name" class="block font-semibold">Name</label>
        <input id="name" v-model="form.name" :class="fieldClass('name')" placeholder="4 Section Plate" :aria-invalid="!!errors.name" />
        <p v-if="errors.name" class="mt-1.5 text-sm text-danger">{{ errors.name }}</p>
      </div>
      <div class="grid gap-5 sm:grid-cols-2">
        <div>
          <label for="mrp" class="block font-semibold">MRP <span class="font-normal text-muted">(optional)</span></label>
          <input id="mrp" v-model="form.mrp" inputmode="decimal" :class="fieldClass('mrp')" placeholder="750" :aria-invalid="!!errors.mrp" />
          <p v-if="errors.mrp" class="mt-1.5 text-sm text-danger">{{ errors.mrp }}</p>
        </div>
        <div>
          <label for="price" class="block font-semibold">Selling price</label>
          <input id="price" v-model="form.sellingPrice" inputmode="decimal" :class="fieldClass('sellingPrice')" placeholder="700" :aria-invalid="!!errors.sellingPrice" />
          <p v-if="errors.sellingPrice" class="mt-1.5 text-sm text-danger">{{ errors.sellingPrice }}</p>
          <p v-else class="mt-1.5 text-sm text-muted">What the counter charges. Can't be more than the MRP.</p>
        </div>
      </div>
    </section>

    <section class="card space-y-5 p-5 sm:p-6">
      <h2 class="text-xl font-bold">In the shop</h2>
      <div>
        <label for="category" class="block font-semibold">Category <span class="font-normal text-muted">(optional)</span></label>
        <div v-if="newCategory === null" class="mt-1.5 flex gap-2">
          <select id="category" v-model="form.categoryId" class="field">
            <option value="">No category</option>
            <option v-for="c in categories" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
          <button type="button" class="btn-secondary shrink-0" @click="newCategory = ''">New category</button>
        </div>
        <div v-else class="mt-1.5 flex gap-2">
          <input v-model="newCategory" class="field" placeholder="e.g. Lunch boxes" aria-label="New category name" @keydown.enter.prevent="addCategory" />
          <button type="button" class="btn-secondary shrink-0" @click="addCategory">Add</button>
          <button type="button" class="btn-secondary shrink-0" @click="newCategory = null">Cancel</button>
        </div>
        <p v-if="categoryError" class="mt-1.5 text-sm text-danger">{{ categoryError }}</p>
      </div>
      <div class="grid gap-5 sm:grid-cols-2">
        <div>
          <label for="unit" class="block font-semibold">Sold by</label>
          <select id="unit" v-model="form.unit" class="field mt-1.5 capitalize">
            <option v-for="u in UNITS" :key="u" :value="u">{{ u }}</option>
          </select>
        </div>
        <div v-if="form.unit !== 'piece'">
          <label for="pack" class="block font-semibold">Pieces per {{ form.unit }}</label>
          <input id="pack" v-model="form.packQty" inputmode="numeric" :class="fieldClass('packQty')" />
          <p v-if="errors.packQty" class="mt-1.5 text-sm text-danger">{{ errors.packQty }}</p>
        </div>
      </div>
      <div v-if="canSeeCost">
        <label for="cost" class="block font-semibold">Cost price <span class="font-normal text-muted">(optional, staff only)</span></label>
        <input id="cost" v-model="form.defaultCost" inputmode="decimal" :class="fieldClass('defaultCost')" />
        <p v-if="errors.defaultCost" class="mt-1.5 text-sm text-danger">{{ errors.defaultCost }}</p>
        <p v-else class="mt-1.5 text-sm text-muted">Cashiers don't see this.</p>
      </div>
      <div>
        <label for="description" class="block font-semibold">Notes <span class="font-normal text-muted">(optional)</span></label>
        <textarea id="description" v-model="form.description" rows="3" :class="fieldClass('description')" />
      </div>
      <div class="sm:w-1/2">
        <label for="hsn" class="block font-semibold">HSN code <span class="font-normal text-muted">(optional, for GST)</span></label>
        <input id="hsn" v-model="form.hsnCode" inputmode="numeric" :class="fieldClass('hsnCode')" />
      </div>
    </section>

    <p v-if="formError" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ formError }}</p>

    <div class="flex flex-wrap gap-2">
      <button type="submit" class="btn-primary" :disabled="saving">
        {{ saving ? 'Saving…' : editing ? 'Save changes' : 'Add product' }}
      </button>
      <RouterLink :to="id ? `/products/${id}` : '/products'" class="btn-secondary">Cancel</RouterLink>
    </div>
  </form>
</template>
