<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from './session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { homeFor, safeNext } from '@/app/guards'
import KitchenwareArt from '@/shared/ui/KitchenwareArt.vue'
import ThemeToggle from '@/shared/ui/ThemeToggle.vue'

const session = useSessionStore()
const router = useRouter()
const route = useRoute()

const username = ref('')
const password = ref('')
const showPassword = ref(false)
const busy = ref(false)
const error = ref<string | null>(null)

async function submit() {
  busy.value = true
  error.value = null
  try {
    await session.login(username.value, password.value)
    const next = safeNext(route.query.next, '')
    await router.replace(next && (!session.isDriverOnly || next.startsWith('/driver')) ? next : homeFor(session))
  } catch (e) {
    error.value = friendlyMessage(e)
    password.value = ''
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="min-h-dvh lg:grid lg:grid-cols-[1.1fr_1fr]">
    <!-- Picture side -->
    <section class="relative overflow-hidden bg-nav text-nav-ink px-6 pt-10 pb-6 lg:flex lg:flex-col lg:justify-between lg:rounded-none lg:p-14">
      <ThemeToggle class="absolute top-3 right-3 text-nav-muted hover:bg-nav-hover hover:text-nav-ink lg:top-5 lg:right-5" />
      <div>
        <p class="display text-[2.6rem] leading-[0.95] font-extrabold lg:text-7xl">Jehovah Jireh</p>
        <p class="mt-3 max-w-xs text-lg text-nav-muted lg:mt-5 lg:text-xl">Shop counter, stock and delivery, in one place.</p>
      </div>
      <KitchenwareArt class="mx-auto mt-6 w-full max-w-72 text-black lg:mt-10 lg:max-w-xl" />
    </section>

    <!-- Form side -->
    <main class="mx-auto w-full max-w-md px-6 pt-8 pb-12 lg:flex lg:flex-col lg:justify-center lg:py-14">
      <h1 class="text-3xl font-extrabold">Welcome back</h1>
      <p class="mt-1 text-muted">Sign in with the username the shop gave you.</p>

      <form class="mt-7 space-y-5" @submit.prevent="submit">
        <div>
          <label for="username" class="block font-semibold">Username</label>
          <input
            id="username"
            v-model="username"
            name="username"
            type="text"
            autocomplete="username"
            autocapitalize="none"
            autocorrect="off"
            spellcheck="false"
            enterkeyhint="next"
            required
            class="field mt-1.5"
          />
        </div>

        <div>
          <label for="current-password" class="block font-semibold">Password</label>
          <div class="relative mt-1.5">
            <input
              id="current-password"
              v-model="password"
              name="password"
              :type="showPassword ? 'text' : 'password'"
              autocomplete="current-password"
              enterkeyhint="done"
              required
              class="field pr-20"
            />
            <button
              type="button"
              class="absolute inset-y-1.5 right-1.5 rounded-[8px] px-3 font-semibold text-primary-ink hover:bg-primary-tint"
              :aria-pressed="showPassword"
              aria-controls="current-password"
              @click="showPassword = !showPassword"
            >
              {{ showPassword ? 'Hide' : 'Show' }}
            </button>
          </div>
        </div>

        <p v-if="error" role="alert" class="rounded-control bg-danger-tint px-4 py-3 font-medium text-danger">
          {{ error }}
        </p>

        <button type="submit" :disabled="busy" class="btn-primary w-full">
          {{ busy ? 'Signing in…' : 'Sign in' }}
        </button>
      </form>

      <p class="mt-8 text-muted">Forgot your password? Ask the shop administrator to reset it.</p>
    </main>
  </div>
</template>
