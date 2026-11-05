<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from '@/modules/auth/session.store'
import { splitForTabBar, visibleNav } from '@/app/nav'
import NavIcon from './NavIcon.vue'
import Avatar from '@/shared/ui/Avatar.vue'
import ThemeToggle from '@/shared/ui/ThemeToggle.vue'

const session = useSessionStore()
const router = useRouter()
const route = useRoute()
const nav = computed(() => visibleNav((p) => session.can(p)))
// A bottom bar with a single tab is noise; phones only show it from two items up.
const showTabBar = computed(() => nav.value.length > 1)
const split = computed(() => splitForTabBar(nav.value, 4))
const moreOpen = ref(false)
const moreActive = computed(() => split.value.more.some((i) => route.path === i.to || route.path.startsWith(i.to + '/')))
watch(() => route.path, () => (moreOpen.value = false))
const name = computed(() => session.user?.display_name ?? '')

async function signOut() {
  await session.logout().catch(() => {})
  await router.replace('/login')
}
</script>

<template>
  <div class="min-h-dvh md:flex">
    <!-- Tablet and desktop: charcoal side menu -->
    <aside class="hidden bg-nav text-nav-ink md:sticky md:top-0 md:flex md:h-dvh md:w-64 md:shrink-0 md:flex-col md:p-4">
      <p class="display px-3 pt-3 pb-6 text-2xl leading-none font-extrabold">
        Jehovah Jireh<span class="text-primary">.</span>
      </p>
      <nav aria-label="Main" class="flex-1">
        <ul class="space-y-1">
          <li v-for="item in nav" :key="item.to">
            <RouterLink
              :to="item.to"
              class="flex items-center gap-3 rounded-control px-3 py-2.5 font-semibold text-nav-muted transition-colors hover:bg-nav-hover hover:text-nav-ink"
              exact-active-class="!bg-primary !text-on-primary"
            >
              <NavIcon :name="item.icon" class="size-5 shrink-0" />
              {{ item.label }}
            </RouterLink>
          </li>
        </ul>
      </nav>
      <ThemeToggle show-label class="mb-3 w-full text-nav-muted hover:bg-nav-hover hover:text-nav-ink" />
      <div class="flex items-center gap-3 border-t border-nav-hover px-1 pt-4">
        <Avatar :name="name" class="size-10" />
        <div class="min-w-0">
          <p class="truncate font-semibold">{{ name }}</p>
          <button type="button" class="text-sm text-nav-muted underline underline-offset-4 hover:text-nav-ink" @click="signOut">Sign out</button>
        </div>
      </div>
    </aside>

    <!-- Phone: charcoal top bar -->
    <header
      class="sticky top-0 z-10 flex items-center justify-between gap-3 bg-nav px-4 pt-[max(0.75rem,env(safe-area-inset-top))] pb-3 text-nav-ink md:hidden"
    >
      <p class="display text-xl font-extrabold">Jehovah Jireh<span class="text-primary">.</span></p>
      <div class="-mr-2 flex items-center gap-1">
        <ThemeToggle class="hover:bg-nav-hover" />
        <button type="button" class="min-h-10 rounded-control px-3 font-semibold hover:bg-nav-hover" @click="signOut">Sign out</button>
      </div>
    </header>

    <main class="flex-1 px-4 py-6 md:px-8 md:py-8" :class="showTabBar ? 'pb-28 md:pb-8' : ''">
      <div class="mx-auto max-w-5xl">
        <RouterView />
      </div>
    </main>

    <!-- Phone: charcoal bottom tabs, within thumb reach -->
    <nav
      v-if="showTabBar"
      aria-label="Main"
      class="fixed inset-x-0 bottom-0 z-10 bg-nav px-2 pt-1.5 pb-[max(0.375rem,env(safe-area-inset-bottom))] md:hidden"
    >
      <div
        v-if="moreOpen && split.more.length"
        id="more-menu"
        class="absolute inset-x-2 bottom-full mb-2 rounded-card border border-nav-hover bg-nav p-1 shadow-card"
      >
        <RouterLink
          v-for="item in split.more"
          :key="item.to"
          :to="item.to"
          class="flex min-h-12 items-center gap-3 rounded-control px-3 font-semibold text-nav-ink hover:bg-nav-hover"
          active-class="!text-primary"
        >
          <NavIcon :name="item.icon" class="size-6" />
          {{ item.label }}
        </RouterLink>
      </div>
      <ul class="flex gap-1">
        <li v-for="item in split.tabs" :key="item.to" class="min-w-0 flex-1">
          <RouterLink
            :to="item.to"
            class="flex min-h-14 flex-col items-center justify-center gap-0.5 rounded-control text-sm font-semibold text-nav-muted"
            exact-active-class="!bg-nav-hover !text-primary"
          >
            <NavIcon :name="item.icon" class="size-6" />
            {{ item.label }}
          </RouterLink>
        </li>
        <li v-if="split.more.length" class="min-w-0 flex-1">
          <button
            type="button"
            class="flex min-h-14 w-full flex-col items-center justify-center gap-0.5 rounded-control text-sm font-semibold"
            :class="moreOpen || moreActive ? 'bg-nav-hover text-primary' : 'text-nav-muted'"
            aria-controls="more-menu"
            :aria-expanded="moreOpen"
            @click="moreOpen = !moreOpen"
          >
            <svg viewBox="0 0 24 24" class="size-6" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true"><path d="M5 12h.01M12 12h.01M19 12h.01" /></svg>
            More
          </button>
        </li>
      </ul>
    </nav>
  </div>
</template>
