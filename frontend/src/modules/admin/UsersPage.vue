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
  <h1 class="text-xl font-semibold">Users</h1>
  <p v-if="error" role="alert" class="mt-4 text-red-700 dark:text-red-300">{{ error }}</p>
  <p v-else-if="!users" class="mt-4 text-slate-500">Loading…</p>
  <div v-else class="mt-4 overflow-x-auto rounded-lg border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900">
    <table class="w-full text-left text-sm">
      <thead class="border-b border-slate-200 text-slate-600 dark:border-slate-800 dark:text-slate-400">
        <tr>
          <th scope="col" class="px-4 py-2 font-medium">Username</th>
          <th scope="col" class="px-4 py-2 font-medium">Name</th>
          <th scope="col" class="px-4 py-2 font-medium">Roles</th>
          <th scope="col" class="px-4 py-2 font-medium">Status</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="u in users" :key="u.id" class="border-b border-slate-100 last:border-0 dark:border-slate-800">
          <td class="px-4 py-2 font-mono">{{ u.username }}</td>
          <td class="px-4 py-2">{{ u.display_name }}</td>
          <td class="px-4 py-2">{{ u.roles.join(', ') }}</td>
          <td class="px-4 py-2">{{ u.is_active ? 'Active' : 'Disabled' }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
