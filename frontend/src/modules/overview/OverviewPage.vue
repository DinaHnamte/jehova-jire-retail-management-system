<script setup lang="ts">
import { useSessionStore } from '@/modules/auth/session.store'
const session = useSessionStore()
</script>

<template>
  <h1 class="text-xl font-semibold">Overview</h1>
  <p class="mt-2 text-slate-600 dark:text-slate-400">Welcome, {{ session.user?.display_name }}.</p>
</template>
