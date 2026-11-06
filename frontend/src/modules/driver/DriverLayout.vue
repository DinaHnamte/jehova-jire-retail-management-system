<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'

const session = useSessionStore()
const router = useRouter()
const name = computed(() => session.user?.display_name ?? '')

async function signOut() {
  await session.logout().catch(() => {})
  await router.replace('/login')
}
</script>

<template>
  <div class="min-h-dvh bg-canvas">
    <header
      class="sticky top-0 z-10 flex items-center justify-between gap-3 bg-nav px-4 pt-[max(0.75rem,env(safe-area-inset-top))] pb-3 text-nav-ink"
    >
      <div class="min-w-0">
        <p class="display text-lg leading-tight font-extrabold">Jehovah Jireh<span class="text-primary">.</span></p>
        <p class="truncate text-sm text-nav-muted">{{ name }}</p>
      </div>
      <button type="button" class="min-h-12 shrink-0 rounded-control px-4 font-semibold hover:bg-nav-hover" @click="signOut">Log out</button>
    </header>
    <main class="mx-auto w-full max-w-lg p-4">
      <RouterView />
    </main>
  </div>
</template>
