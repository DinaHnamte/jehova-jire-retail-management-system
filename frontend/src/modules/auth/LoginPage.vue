<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useSessionStore } from './session.store'
import { friendlyMessage } from '@/shared/api/errors'
import { safeNext } from '@/app/guards'

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
    await router.replace(safeNext(route.query.next))
  } catch (e) {
    error.value = friendlyMessage(e)
    password.value = ''
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="min-h-dvh sm:grid sm:place-items-center sm:p-6">
    <div class="sm:w-full sm:max-w-md sm:overflow-hidden sm:rounded-xl sm:border-2 sm:border-line sm:bg-surface">
      <header class="bg-brand px-6 pt-14 pb-6 text-on-brand sm:pt-10">
        <p class="text-[2.75rem] leading-[0.95] font-extrabold tracking-tight">Jehovah<br />Jireh</p>
        <p class="mt-3 text-base opacity-85">Shop counter, stock and delivery</p>
      </header>

      <main class="px-6 pt-8 pb-10">
        <h1 class="text-2xl font-bold">Sign in</h1>

        <form class="mt-5 space-y-5" @submit.prevent="submit">
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
                class="absolute inset-y-0 right-0 px-4 font-semibold text-action"
                :aria-pressed="showPassword"
                aria-controls="current-password"
                @click="showPassword = !showPassword"
              >
                {{ showPassword ? 'Hide' : 'Show' }}
              </button>
            </div>
          </div>

          <p v-if="error" role="alert" class="rounded-md border-l-4 border-error bg-error-tint px-4 py-3 text-ink">
            {{ error }}
          </p>

          <button type="submit" :disabled="busy" class="btn-primary w-full">
            {{ busy ? 'Signing in…' : 'Sign in' }}
          </button>
        </form>

        <p class="mt-8 text-muted">Forgot your password? Ask the shop administrator to reset it.</p>
      </main>
    </div>
  </div>
</template>
