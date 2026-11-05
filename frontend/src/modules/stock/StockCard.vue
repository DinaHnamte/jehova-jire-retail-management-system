<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useSessionStore } from '@/modules/auth/session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { formatRupees } from '@/shared/money'
import Badge from '@/shared/ui/Badge.vue'
import type { Page } from '@/modules/products/api'
import AdjustDialog from './AdjustDialog.vue'
import { getProductStock, listMovements, type Movement, type ProductStock } from './api'
import { formatCost, formatWhen, movementLabel, signed } from './format'

const props = defineProps<{ productId: string; archived: boolean }>()
const session = useSessionStore()
const canAdjust = computed(() => session.can('inventory.adjust') && !props.archived)
const seesCost = computed(() => session.can('products.update'))

const stock = ref<ProductStock | null>(null)
const history = ref<Page<Movement> | null>(null)
const page = ref(1)
const error = ref<string | null>(null)
const adjust = ref<InstanceType<typeof AdjustDialog> | null>(null)
const lastPage = computed(() => (history.value ? Math.max(1, Math.ceil(history.value.total / history.value.per_page)) : 1))

async function load() {
  error.value = null
  try {
    ;[stock.value, history.value] = await Promise.all([
      getProductStock(props.productId),
      listMovements(props.productId, page.value),
    ])
  } catch (e) {
    error.value = friendlyMessage(e)
  }
}
onMounted(load)

function goTo(p: number) {
  page.value = p
  load()
}

function onSaved() {
  page.value = 1
  load()
}
</script>

<template>
  <section class="card mt-4 p-5 sm:p-6" aria-labelledby="stock-h">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <h2 id="stock-h" class="text-xl font-bold">Stock</h2>
      <button v-if="canAdjust && stock" type="button" class="btn-secondary" @click="adjust?.open()">Adjust stock</button>
    </div>

    <p v-if="error" role="alert" class="mt-4 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>

    <template v-else-if="stock">
      <dl class="mt-4 grid grid-cols-2 gap-4 sm:grid-cols-3">
        <div>
          <dt class="text-sm text-muted">On hand</dt>
          <dd class="figure mt-1 text-4xl">{{ stock.on_hand }}</dd>
        </div>
        <div v-if="seesCost">
          <dt class="text-sm text-muted">Average cost</dt>
          <dd class="mt-1">
            <span v-if="stock.avg_cost" class="figure text-2xl">{{ formatCost(stock.avg_cost) }}</span>
            <Badge v-else-if="stock.on_hand > 0" tone="golden">Cost unknown</Badge>
            <span v-else class="text-muted">—</span>
          </dd>
        </div>
        <div v-if="seesCost">
          <dt class="text-sm text-muted">Stock value</dt>
          <dd class="figure mt-1 text-2xl">{{ formatRupees(stock.stock_value) }}</dd>
        </div>
      </dl>

      <h3 class="mt-6 font-bold">History</h3>
      <p v-if="history && history.total === 0" class="mt-2 text-muted">
        No stock recorded yet. Count it in a stock count, or receive it from a supplier.
      </p>
      <ul v-else-if="history" class="mt-2 divide-y divide-line">
        <li v-for="m in history.items" :key="m.id" class="grid grid-cols-[1fr_auto] gap-x-4 py-3">
          <div>
            <p class="font-semibold">
              {{ movementLabel(m.movement_type) }}
              <span v-if="m.unit_cost" class="font-normal text-muted">at {{ formatRupees(m.unit_cost) }} each</span>
            </p>
            <p class="text-sm text-muted">
              {{ formatWhen(m.created_at) }}, {{ m.created_by_name }}<template v-if="m.reason">. {{ m.reason }}</template>
            </p>
          </div>
          <span class="figure self-center text-xl" :class="m.quantity < 0 ? 'text-danger' : 'text-sage-ink'">{{ signed(m.quantity) }}</span>
        </li>
      </ul>
      <nav v-if="history && history.total > history.per_page" aria-label="History pages" class="mt-3 flex justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="page <= 1" @click="goTo(page - 1)">Newer</button>
        <button type="button" class="btn-secondary" :disabled="page >= lastPage" @click="goTo(page + 1)">Older</button>
      </nav>
    </template>

    <p v-else class="mt-4 text-muted">Loading stock…</p>

    <AdjustDialog v-if="stock" ref="adjust" :product-id="productId" :on-hand="stock.on_hand" @saved="onSaved" />
  </section>
</template>
