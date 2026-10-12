import { createApp } from "vue"
import { createPinia } from "pinia"
import "./style.css"
import App from "./App.vue"
import InternalBrowser from "./components/browser/InternalBrowser.vue"
import i18n from "./locales"
import { tooltip } from "./directives/tooltip"
import { invoke } from '@tauri-apps/api/core'
import { handleExternalLinkClick } from './utils/externalLinks'

const openExternalLink = (event) => handleExternalLinkClick(
  event,
  url => invoke('open_url', { url }),
  error => {
    console.error('Failed to open external link:', error)
    window.$notify?.error(i18n.global.t('messages.openUrlFailed'))
  }
)
window.addEventListener('click', openExternalLink)
if (import.meta.hot) import.meta.hot.dispose(() => window.removeEventListener('click', openExternalLink))

const THEME_STORAGE_KEY = "atm-theme"
const prefersDarkMedia = window.matchMedia("(prefers-color-scheme: dark)")

const applyTheme = (theme) => {
  const normalized = theme === "dark" ? "dark" : "light"
  const root = document.documentElement
  root.dataset.theme = normalized
  root.style.colorScheme = normalized
}

const getStoredTheme = () => {
  try {
    return localStorage.getItem(THEME_STORAGE_KEY)
  } catch (error) {
    console.warn("Failed to read stored theme preference", error)
    return null
  }
}

const resolveSystemTheme = () => (prefersDarkMedia.matches ? "dark" : "light")

const resolveInitialTheme = () => {
  const stored = getStoredTheme()
  if (stored === "dark" || stored === "light") {
    return stored
  }
  return resolveSystemTheme()
}

const initialTheme = resolveInitialTheme()
applyTheme(initialTheme)

const themeManager = {
  storageKey: THEME_STORAGE_KEY,
  mediaQuery: prefersDarkMedia,
  applyTheme,
  getStoredTheme,
  resolveSystemTheme,
  initialTheme
}

const pinia = createPinia()
const root = new URLSearchParams(window.location.search).get('window') === 'browser' ? InternalBrowser : App
const app = createApp(root)
app.use(pinia)
app.use(i18n)
app.directive('tooltip', tooltip)
app.provide("themeManager", themeManager)
app.mount("#app")
