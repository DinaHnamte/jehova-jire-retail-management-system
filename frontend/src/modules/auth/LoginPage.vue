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
  <main class="mx-auto flex min-h-dvh max-w-sm flex-col px-4 pt-12 sm:pt-24">
    <h1 class="text-2xl font-semibold">Sign in</h1>
    <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">Jehovah Jireh</p>

    <form class="mt-6 space-y-4" @submit.prevent="submit">
      <div>
        <label for="username" class="block text-sm font-medium">Username</label>
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
          class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2.5 text-base focus:border-brand-600 focus:outline-2 focus:outline-brand-600 dark:border-slate-700 dark:bg-slate-900"
        />
      </div>

      <div>
        <label for="current-password" class="block text-sm font-medium">Password</label>
        <div class="relative mt-1">
          <input
            id="current-password"
            v-model="password"
            name="password"
            :type="showPassword ? 'text' : 'password'"
            autocomplete="current-password"
            enterkeyhint="done"
            required
            class="block w-full rounded-lg border border-slate-300 bg-white px-3 py-2.5 pr-20 text-base focus:border-brand-600 focus:outline-2 focus:outline-brand-600 dark:border-slate-700 dark:bg-slate-900"
          />
          <button
            type="button"
            class="absolute inset-y-0 right-0 px-3 text-sm font-medium text-brand-600 dark:text-blue-400"
            :aria-pressed="showPassword"
            aria-controls="current-password"
            @click="showPassword = !showPassword"
          >
            {{ showPassword ? 'Hide' : 'Show' }}
          </button>
        </div>
      </div>

      <p v-if="error" role="alert" class="rounded-lg bg-red-50 px-3 py-2 text-sm text-red-800 dark:bg-red-950 dark:text-red-200">
        {{ error }}
      </p>

      <button
        type="submit"
        :disabled="busy"
        class="w-full rounded-lg bg-brand-600 px-4 py-2.5 text-base font-semibold text-white hover:bg-brand-700 disabled:opacity-60"
      >
        {{ busy ? 'Signing in…' : 'Sign in' }}
      </button>
    </form>

    <p class="mt-6 text-sm text-slate-600 dark:text-slate-400">
      Forgot your password? Ask the shop administrator to reset it.
    </p>
  </main>
</template>
