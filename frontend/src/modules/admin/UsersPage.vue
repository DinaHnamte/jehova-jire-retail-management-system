<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api } from '@/shared/api/client'
import { friendlyMessage } from '@/shared/api/errors'
import Avatar from '@/shared/ui/Avatar.vue'
import Badge, { type BadgeTone } from '@/shared/ui/Badge.vue'
import { useSessionStore } from '@/modules/auth/session.store'
import { formatRupees } from '@/shared/money'
import DriverDialog from './DriverDialog.vue'
import { listDrivers, updateDriver, type Driver } from './drivers-api'

interface UserRow {
  id: string
  username: string
  display_name: string
  is_active: boolean
  roles: string[]
}

const roles: Record<string, { name: string; tone: BadgeTone }> = {
  admin: { name: 'Admin', tone: 'primary' },
  manager: { name: 'Manager', tone: 'dark' },
  cashier: { name: 'Cashier', tone: 'golden' },
  stock_operator: { name: 'Stock', tone: 'clay' },
  delivery_driver: { name: 'Driver', tone: 'sage' },
  auditor: { name: 'Auditor', tone: 'neutral' },
}
const role = (code: string) => roles[code] ?? { name: code, tone: 'neutral' as BadgeTone }

const users = ref<UserRow[] | null>(null)
const error = ref<string | null>(null)

const session = useSessionStore()
const canDrivers = session.can('drivers.manage')
const drivers = ref<Driver[] | null>(null)
const driverError = ref<string | null>(null)
const dialog = ref<InstanceType<typeof DriverDialog> | null>(null)
const confirming = ref<Driver | null>(null)
const confirmDialog = ref<HTMLDialogElement | null>(null)
const busyId = ref<string | null>(null)

async function loadDrivers() {
  try {
    drivers.value = await listDrivers()
  } catch (e) {
    driverError.value = friendlyMessage(e)
  }
}

async function setActive(d: Driver, active: boolean) {
  busyId.value = d.id
  driverError.value = null
  try {
    await updateDriver(d.id, { is_active: active })
    await loadDrivers()
  } catch (e) {
    driverError.value = friendlyMessage(e)
  } finally {
    busyId.value = null
  }
}

function toggle(d: Driver) {
  if (d.is_active && d.open_jobs > 0) {
    confirming.value = d
    confirmDialog.value?.showModal()
  } else void setActive(d, !d.is_active)
}

function confirmDeactivate() {
  const d = confirming.value
  confirmDialog.value?.close()
  if (d) void setActive(d, false)
}

onMounted(async () => {
  if (canDrivers) void loadDrivers()
  try {
    users.value = await api.get<UserRow[]>('/users')
  } catch (e) {
    error.value = friendlyMessage(e)
  }
})
</script>

<template>
  <h1 class="text-3xl font-extrabold">Users</h1>
  <p class="mt-1 text-muted">Everyone who can sign in, and what they're allowed to do.</p>

  <section v-if="canDrivers" class="mt-6" aria-labelledby="drivers-h">
    <div class="flex items-center justify-between gap-3">
      <h2 id="drivers-h" class="text-xl font-bold">Drivers</h2>
      <button type="button" class="btn-primary" @click="dialog?.open('add')">Add driver</button>
    </div>
    <p v-if="driverError" role="alert" class="mt-3 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ driverError }}</p>
    <p v-if="!drivers && !driverError" class="mt-3 text-muted">Loading drivers…</p>
    <p v-else-if="drivers && !drivers.length" class="mt-3 text-muted">No drivers yet. Add one so deliveries can be assigned.</p>
    <ul v-else-if="drivers" class="card mt-3 overflow-hidden">
      <li v-for="d in drivers" :key="d.id" class="border-b border-line px-5 py-4 last:border-0">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <p class="truncate font-bold">{{ d.display_name }}</p>
            <p class="text-sm text-muted">{{ d.phone }} · {{ formatRupees(d.default_pay) }} per trip</p>
            <p class="text-sm text-muted">{{ d.open_jobs }} open {{ d.open_jobs === 1 ? 'delivery' : 'deliveries' }}</p>
          </div>
          <Badge :tone="d.is_active ? 'sage' : 'danger'" dot>{{ d.is_active ? 'Active' : 'Inactive' }}</Badge>
        </div>
        <div class="mt-3 flex flex-wrap gap-2">
          <button type="button" class="btn-secondary" @click="dialog?.open('edit', d)">Edit</button>
          <button type="button" class="btn-secondary" @click="dialog?.open('password', d)">Reset password</button>
          <button type="button" class="btn-secondary" :disabled="busyId === d.id" @click="toggle(d)">
            {{ d.is_active ? 'Deactivate' : 'Reactivate' }}
          </button>
        </div>
      </li>
    </ul>
    <DriverDialog ref="dialog" @saved="loadDrivers" />
    <dialog
      ref="confirmDialog"
      aria-labelledby="deact-title"
      class="m-auto w-[min(100%-2rem,28rem)] rounded-card border border-line bg-surface p-0 text-ink shadow-card backdrop:bg-ink/40"
    >
      <div class="p-5 sm:p-6">
        <h2 id="deact-title" class="text-xl font-bold">Deactivate {{ confirming?.display_name }}?</h2>
        <p class="mt-2 text-muted">
          {{ confirming?.display_name }} has {{ confirming?.open_jobs }} open deliveries. They stay assigned until you reassign them.
        </p>
        <div class="mt-6 flex justify-end gap-2">
          <button type="button" class="btn-secondary" @click="confirmDialog?.close()">Keep active</button>
          <button type="button" class="btn-primary" @click="confirmDeactivate">Deactivate</button>
        </div>
      </div>
    </dialog>
  </section>

  <h2 v-if="canDrivers" class="mt-8 text-xl font-bold">Staff</h2>
  <p v-if="error" role="alert" class="mt-6 rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">{{ error }}</p>
  <p v-else-if="!users" class="mt-6 text-muted">Loading users…</p>
  <div v-else class="card mt-6 overflow-hidden">
    <table class="w-full text-left">
      <thead class="hidden border-b border-line text-sm text-muted sm:table-header-group">
        <tr>
          <th scope="col" class="px-5 py-3 font-semibold">Name</th>
          <th scope="col" class="px-5 py-3 font-semibold">Roles</th>
          <th scope="col" class="px-5 py-3 text-right font-semibold">Status</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="u in users"
          :key="u.id"
          class="grid grid-cols-[1fr_auto] gap-x-4 gap-y-3 border-b border-line px-5 py-4 last:border-0 sm:table-row sm:p-0"
        >
          <td class="sm:px-5 sm:py-5">
            <div class="flex items-center gap-3">
              <Avatar :name="u.display_name" class="size-11" />
              <div class="min-w-0">
                <p class="truncate font-bold">{{ u.display_name }}</p>
                <p class="truncate text-sm text-muted">{{ u.username }}</p>
              </div>
            </div>
          </td>
          <td class="col-span-2 row-start-2 sm:px-5 sm:py-5">
            <span class="flex flex-wrap gap-1.5">
              <Badge v-for="r in u.roles" :key="r" :tone="role(r).tone">{{ role(r).name }}</Badge>
            </span>
          </td>
          <td class="self-center text-right sm:px-5 sm:py-5">
            <Badge :tone="u.is_active ? 'sage' : 'danger'" dot>{{ u.is_active ? 'Active' : 'Disabled' }}</Badge>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
