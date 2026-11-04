import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './app/router'
import { configureApi } from './shared/api/client'
import { useSessionStore } from './modules/auth/session.store'
import './style.css'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)

const session = useSessionStore(pinia)
configureApi({
  getCsrfToken: () => session.csrfToken,
  onUnauthenticated: () => {
    session.clear()
    const current = router.currentRoute.value
    if (!current.meta.public) router.replace({ path: '/login', query: { next: current.fullPath } })
  },
})

app.use(router)
app.mount('#app')
