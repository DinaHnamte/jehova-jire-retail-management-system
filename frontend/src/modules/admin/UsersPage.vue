<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api } from '@/shared/api/client'
import { friendlyMessage } from '@/shared/api/errors'

interface UserRow {
  id: string
  username: string
  display_name: string
  is_active: boolean
  roles: string[]
}

const roleNames: Record<string, string> = {
  admin: 'Admin',
  manager: 'Manager',
  cashier: 'Cashier',
  stock_operator: 'Stock',
  delivery_driver: 'Driver',
  auditor: 'Auditor',
}
const roleName = (code: string) => roleNames[code] ?? code

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
  <h1 class="text-2xl font-bold">Users</h1>
  <p class="mt-1 text-muted">Everyone who can sign in, and what they're allowed to do.</p>

  <p v-if="error" role="alert" class="mt-6 rounded-md border-l-4 border-error bg-error-tint px-4 py-3">{{ error }}</p>
  <p v-else-if="!users" class="mt-6 text-muted">Loading users…</p>
  <ul v-else class="mt-6 max-w-3xl divide-y-2 divide-line overflow-hidden rounded-lg border-2 border-line bg-surface">
    <li v-for="u in users" :key="u.id" class="grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-2 px-4 py-4 sm:grid-cols-[1fr_auto_6rem]">
      <div class="min-w-0">
        <p class="truncate font-bold">{{ u.display_name }}</p>
        <p class="truncate text-muted">{{ u.username }}</p>
      </div>
      <p
        class="text-right font-semibold sm:order-last"
        :class="u.is_active ? 'text-paid' : 'text-muted line-through'"
      >
        {{ u.is_active ? 'Active' : 'Disabled' }}
      </p>
      <ul class="col-span-2 flex flex-wrap gap-1.5 sm:col-span-1 sm:justify-end" aria-label="Roles">
        <li v-for="r in u.roles" :key="r" class="rounded-md bg-action-tint px-2 py-0.5 text-sm font-semibold text-action">
          {{ roleName(r) }}
        </li>
      </ul>
    </li>
  </ul>
</template>
