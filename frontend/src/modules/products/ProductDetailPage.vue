<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import StockCard from '@/modules/stock/StockCard.vue'
import { addIdentifier, archiveProduct, getProduct, removeIdentifier, restoreProduct, type Product } from './api'

const route = useRoute()
const session = useSessionStore()
const id = route.params.id as string
const canUpdate = computed(() => session.can('products.update'))
const canArchive = computed(() => session.can('products.archive'))
const canSeeStock = computed(() => session.can('inventory.read'))

const product = ref<Product | null>(null)
const error = ref<string | null>(null)
const actionError = ref<string | null>(null)
const confirmArchive = ref(false)
const busy = ref(false)

const newType = ref<'alias' | 'sku'>('alias')
const newValue = ref('')
const otherCodes = computed(() => product.value?.identifiers.filter((i) => i.type !== 'product_code') ?? [])

onMounted(async () => {
  try {
    product.value = await getProduct(id)
  } catch (e) {
    error.value = friendlyMessage(e)
  }
})

async function run(fn: () => Promise<unknown>) {
  actionError.value = null
  busy.value = true
  try {
    await fn()
    product.value = await getProduct(id)
  } catch (e) {
    actionError.value = friendlyMessage(e)
  } finally {
    busy.value = false
  }
}

const toggleArchive = () =>
  run(async () => {
    await (product.value?.archived ? restoreProduct(id) : archiveProduct(id))
    confirmArchive.value = false
  })

const add = () =>
  run(async () => {
    if (!newValue.value.trim()) return
    await addIdentifier(id, newType.value, newValue.value)
    newValue.value = ''
  })
</script>

<template>
  <RouterLink to="/products" class="text-muted hover:text-ink">← Products</RouterLink>

  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
  <p v-else-if="!product" class="mt-6 text-muted">Loading…</p>

  <template v-else>
    <div class="mt-3 flex flex-wrap items-start justify-between gap-4">
      <div class="min-w-0">
        <h1 class="text-3xl font-extrabold">{{ product.name }}</h1>
        <p class="mt-1 flex flex-wrap items-center gap-2">
          <span class="figure text-2xl">{{ product.code ?? 'No code' }}</span>
          <Badge v-if="product.design_code" tone="clay">Design {{ product.design_code }}</Badge>
          <Badge v-if="product.archived" tone="neutral" dot>Archived</Badge>
        </p>
      </div>
      <div class="flex flex-wrap gap-2">
        <RouterLink v-if="canUpdate" :to="`/products/${id}/edit`" class="btn-primary">Edit</RouterLink>
        <button
          v-if="canArchive && !confirmArchive"
          type="button"
          class="btn-secondary"
          :disabled="busy"
          @click="product.archived ? toggleArchive() : (confirmArchive = true)"
        >
          {{ product.archived ? 'Restore' : 'Archive' }}
        </button>
      </div>
    </div>

    <div v-if="confirmArchive" class="card mt-4 border-golden bg-golden-tint p-4" role="alertdialog" aria-labelledby="archive-q">
      <p id="archive-q" class="font-semibold text-golden-ink">Archive {{ product.name }}?</p>
      <p class="mt-1 text-golden-ink">It disappears from search and the counter. Its code stays reserved, and you can restore it later.</p>
      <div class="mt-3 flex gap-2">
        <button type="button" class="btn-primary" :disabled="busy" @click="toggleArchive">Archive product</button>
        <button type="button" class="btn-secondary" @click="confirmArchive = false">Cancel</button>
      </div>
    </div>

    <p v-if="actionError" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ actionError }}</p>

    <div class="mt-6 grid gap-4 md:grid-cols-[1fr_1fr]">
      <section class="card p-5 sm:p-6" aria-label="Prices">
        <p class="text-muted">Selling price</p>
        <p class="figure mt-1 text-5xl text-primary">{{ formatRupees(product.selling_price) }}</p>
        <dl class="mt-5 grid grid-cols-2 gap-4 border-t border-line pt-4">
          <div>
            <dt class="text-sm text-muted">MRP</dt>
            <dd class="figure mt-0.5 text-xl">{{ formatRupees(product.mrp) }}</dd>
          </div>
          <div v-if="canUpdate">
            <dt class="text-sm text-muted">Cost price</dt>
            <dd class="figure mt-0.5 text-xl">{{ formatRupees(product.default_cost) }}</dd>
          </div>
        </dl>
      </section>

      <section class="card p-5 sm:p-6" aria-label="Details">
        <dl class="grid grid-cols-2 gap-x-4 gap-y-4">
          <div>
            <dt class="text-sm text-muted">Category</dt>
            <dd class="font-semibold">{{ product.category?.name ?? 'None' }}</dd>
          </div>
          <div>
            <dt class="text-sm text-muted">Brand</dt>
            <dd class="font-semibold">{{ product.brand.name }}</dd>
          </div>
          <div>
            <dt class="text-sm text-muted">Sold by</dt>
            <dd class="font-semibold capitalize">
              {{ product.unit }}<template v-if="product.pack_qty > 1"> of {{ product.pack_qty }}</template>
            </dd>
          </div>
          <div v-if="product.hsn_code">
            <dt class="text-sm text-muted">HSN code</dt>
            <dd class="font-semibold">{{ product.hsn_code }}</dd>
          </div>
          <div v-if="product.description" class="col-span-2">
            <dt class="text-sm text-muted">Notes</dt>
            <dd class="whitespace-pre-line">{{ product.description }}</dd>
          </div>
        </dl>
      </section>
    </div>

    <StockCard v-if="canSeeStock" :product-id="id" :archived="product.archived" />

    <section class="card mt-4 p-5 sm:p-6">
      <h2 class="text-xl font-bold">Other names and codes</h2>
      <p class="mt-1 text-muted">Search also finds the product by these, e.g. what customers call it.</p>
      <ul v-if="otherCodes.length" class="mt-4 divide-y divide-line">
        <li v-for="i in otherCodes" :key="i.id" class="flex items-center justify-between gap-3 py-2.5">
          <span class="flex items-center gap-2">
            <Badge :tone="i.type === 'sku' ? 'clay' : 'sage'">{{ i.type === 'sku' ? 'Shop code' : 'Also called' }}</Badge>
            <span class="font-semibold">{{ i.raw_value }}</span>
          </span>
          <button v-if="canUpdate" type="button" class="text-sm font-semibold text-danger hover:underline" :disabled="busy" @click="run(() => removeIdentifier(id, i.id))">
            Remove
          </button>
        </li>
      </ul>
      <p v-else class="mt-4 text-muted">None yet.</p>
      <form v-if="canUpdate" class="mt-4 flex flex-wrap gap-2" @submit.prevent="add">
        <select v-model="newType" class="field w-auto" aria-label="Kind">
          <option value="alias">Also called</option>
          <option value="sku">Shop code</option>
        </select>
        <input v-model="newValue" class="field min-w-40 flex-1" placeholder="e.g. thali" aria-label="Name or code" />
        <button type="submit" class="btn-secondary" :disabled="busy || !newValue.trim()">Add</button>
      </form>
    </section>
  </template>
</template>
