<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { visibleNav } from '@/app/nav'
import NavIcon from './NavIcon.vue'

const session = useSessionStore()
const router = useRouter()
const nav = computed(() => visibleNav((p) => session.can(p)))
// A bottom bar with a single tab is noise; phones only show it from two items up.
const showTabBar = computed(() => nav.value.length > 1)

async function signOut() {
  await session.logout().catch(() => {})
  await router.replace('/login')
}
</script>

<template>
  <div class="min-h-dvh md:flex">
    <!-- Tablet and desktop: side menu -->
    <aside class="hidden md:sticky md:top-0 md:flex md:h-dvh md:w-60 md:shrink-0 md:flex-col md:border-r-2 md:border-line md:bg-surface">
      <div class="bg-brand px-5 py-5 text-on-brand">
        <p class="text-2xl leading-none font-extrabold tracking-tight">Jehovah Jireh</p>
      </div>
      <nav aria-label="Main" class="flex-1 p-3">
        <ul class="space-y-1">
          <li v-for="item in nav" :key="item.to">
            <RouterLink
              :to="item.to"
              class="flex items-center gap-3 rounded-md px-3 py-2.5 font-semibold text-muted hover:bg-action-tint hover:text-ink"
              exact-active-class="!bg-action-tint !text-action"
            >
              <NavIcon :name="item.icon" class="size-5 shrink-0" />
              {{ item.label }}
            </RouterLink>
          </li>
        </ul>
      </nav>
      <div class="border-t-2 border-line px-5 py-4">
        <p class="truncate font-semibold">{{ session.user?.display_name }}</p>
        <button type="button" class="mt-1 text-muted underline underline-offset-4 hover:text-ink" @click="signOut">Sign out</button>
      </div>
    </aside>

    <!-- Phone: top bar -->
    <header class="sticky top-0 z-10 flex items-center justify-between gap-3 bg-brand px-4 pt-[max(0.75rem,env(safe-area-inset-top))] pb-3 text-on-brand md:hidden">
      <p class="text-xl font-extrabold tracking-tight">Jehovah Jireh</p>
      <button type="button" class="-mr-2 min-h-11 px-2 font-semibold" @click="signOut">Sign out</button>
    </header>

    <main class="flex-1 px-4 py-6 md:px-10 md:py-10" :class="showTabBar ? 'pb-28 md:pb-10' : ''">
      <div class="mx-auto max-w-5xl">
        <RouterView />
      </div>
    </main>

    <!-- Phone: bottom tabs, within thumb reach -->
    <nav
      v-if="showTabBar"
      aria-label="Main"
      class="fixed inset-x-0 bottom-0 z-10 border-t-2 border-line bg-surface pb-[env(safe-area-inset-bottom)] md:hidden"
    >
      <ul class="flex">
        <li v-for="item in nav" :key="item.to" class="flex-1">
          <RouterLink
            :to="item.to"
            class="flex min-h-16 flex-col items-center justify-center gap-1 text-sm font-semibold text-muted"
            exact-active-class="!text-action"
          >
            <NavIcon :name="item.icon" class="size-6" />
            {{ item.label }}
          </RouterLink>
        </li>
      </ul>
    </nav>
  </div>
</template>
