<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api } from '@/shared/api/client'
import { friendlyMessage } from '@/shared/api/errors'
import Avatar from '@/shared/ui/Avatar.vue'
import Badge, { type BadgeTone } from '@/shared/ui/Badge.vue'

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

onMounted(async () => {
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
