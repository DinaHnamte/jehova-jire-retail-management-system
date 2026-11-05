import { ref } from 'vue'

export type Theme = 'light' | 'dark'

/** Same key as public/theme-init.js, which applies the theme before the app loads. */
export const STORAGE_KEY = 'jj-theme'

/** A saved choice wins; otherwise follow the device's setting. */
export function resolveTheme(saved: string | null, systemDark: boolean): Theme {
  if (saved === 'light' || saved === 'dark') return saved
  return systemDark ? 'dark' : 'light'
}

function readSaved(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY)
  } catch {
    return null
  }
}

function systemQuery(): MediaQueryList | null {
  return typeof window.matchMedia === 'function' ? window.matchMedia('(prefers-color-scheme: dark)') : null
}

/** Current theme, shared by every toggle on the page. */
export const theme = ref<Theme>(resolveTheme(readSaved(), systemQuery()?.matches ?? false))

function apply(t: Theme) {
  theme.value = t
  document.documentElement.dataset.theme = t
}

/** The person chose a theme: show it and remember it on this device. */
export function setTheme(t: Theme) {
  apply(t)
  try {
    localStorage.setItem(STORAGE_KEY, t)
  } catch {
    // Private mode or blocked storage: the choice lasts until the page closes.
  }
}

export function toggleTheme() {
  setTheme(theme.value === 'dark' ? 'light' : 'dark')
}

// Until someone picks a theme, keep following the device as it changes (e.g. at sunset).
systemQuery()?.addEventListener('change', (e) => {
  if (readSaved() === null) apply(e.matches ? 'dark' : 'light')
})
