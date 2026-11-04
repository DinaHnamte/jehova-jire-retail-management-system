<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { visibleNav } from '@/app/nav'

const session = useSessionStore()
const router = useRouter()
const nav = computed(() => visibleNav((p) => session.can(p)))

async function signOut() {
  await session.logout().catch(() => {})
  await router.replace('/login')
}
</script>

<template>
  <div class="min-h-dvh md:flex">
    <aside class="border-b border-slate-200 bg-white md:w-56 md:shrink-0 md:border-r md:border-b-0 dark:border-slate-800 dark:bg-slate-900">
      <div class="flex items-center justify-between px-4 py-3">
        <span class="font-semibold">Jehovah Jireh</span>
        <button type="button" class="text-sm text-slate-600 md:hidden dark:text-slate-400" @click="signOut">Sign out</button>
      </div>
      <nav aria-label="Main" class="overflow-x-auto">
        <ul class="flex gap-1 px-2 pb-2 md:flex-col">
          <li v-for="item in nav" :key="item.to">
            <RouterLink
              :to="item.to"
              class="block rounded-md px-3 py-2 text-sm whitespace-nowrap hover:bg-slate-100 dark:hover:bg-slate-800"
              exact-active-class="bg-slate-100 font-medium dark:bg-slate-800"
            >
              {{ item.label }}
            </RouterLink>
          </li>
        </ul>
      </nav>
      <div class="hidden border-t border-slate-200 px-4 py-3 text-sm md:block dark:border-slate-800">
        <div class="truncate font-medium">{{ session.user?.display_name }}</div>
        <button type="button" class="mt-1 text-slate-600 hover:underline dark:text-slate-400" @click="signOut">Sign out</button>
      </div>
    </aside>
    <main class="flex-1 px-4 py-6 md:px-8">
      <RouterView />
    </main>
  </div>
</template>
