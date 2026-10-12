<template>
  <main class="flex h-screen flex-col bg-surface text-text" @keydown="handleKeydown">
    <header ref="headerRef" class="shrink-0 border-b border-border bg-surface">
      <div class="browser-toolbar">
        <button v-for="action in navigationActions" :key="action.id" type="button" class="browser-button" :title="t(`internalBrowser.${action.id}`)" :aria-label="t(`internalBrowser.${action.id}`)" :disabled="!ready" @click="runAction(action.id)">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="action.path" /></svg>
        </button>
        <button type="button" class="browser-button" :title="t(state.loading ? 'internalBrowser.stop' : 'internalBrowser.reload')" :aria-label="t(state.loading ? 'internalBrowser.stop' : 'internalBrowser.reload')" :disabled="!ready" @click="runAction(state.loading ? 'stop' : 'reload')">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="state.loading ? 'm6 6 12 12M6 18 18 6' : 'M23 4v6h-6M20.49 15a9 9 0 1 1-2.12-9.36L23 10'" /></svg>
        </button>
        <form class="browser-address mx-1 flex h-8 min-w-0 flex-1 items-center gap-2 rounded-md border border-border bg-muted px-2.5 focus-within:border-border-strong" @submit.prevent="navigate">
          <svg class="h-3.5 w-3.5 shrink-0 text-text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a18 18 0 0 1 0 18 18 18 0 0 1 0-18"/></svg>
          <input ref="addressRef" v-model="address" class="min-w-0 flex-1 bg-transparent text-[13px] outline-none" :aria-label="t('internalBrowser.address')" :placeholder="t('internalBrowser.addressPlaceholder')" spellcheck="false" autocomplete="off" @focus="editingAddress = true" @blur="editingAddress = false" @keydown.esc.prevent="restoreAddress" />
          <span v-if="feedback" role="status" class="max-w-[50%] truncate text-[11px] text-text-secondary" :title="feedback">{{ feedback }}</span>
          <button type="submit" class="rounded px-1 text-[12px] text-text-secondary hover:text-text focus-visible:outline-2 focus-visible:outline-accent" :disabled="!ready" :aria-label="t('internalBrowser.go')">↵</button>
        </form>
        <button v-for="action in utilityActions" :key="action.id" type="button" class="browser-button" :title="t(`internalBrowser.${action.id}`)" :aria-label="t(`internalBrowser.${action.id}`)" :disabled="!ready" @click="runAction(action.id)">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="action.path" /></svg>
        </button>
        <button type="button" class="browser-button relative" :class="showDownloads ? 'bg-hover' : ''" :title="t('internalBrowser.downloads')" :aria-label="t('internalBrowser.downloads')" :aria-pressed="showDownloads" :disabled="!ready" @click="toggleDownloads">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5" /></svg>
          <span v-if="activeDownloads" class="absolute right-0 top-0 h-2 w-2 rounded-full bg-accent" aria-hidden="true"></span>
        </button>
      </div>
      <div class="h-0.5 overflow-hidden bg-border" aria-hidden="true"><div v-if="state.loading" class="browser-progress h-full w-1/3 bg-accent"></div></div>
      <div v-if="errorMessage" role="alert" class="flex flex-wrap items-center justify-between gap-3 border-t border-border bg-danger-muted px-4 py-2 text-[12px] text-danger">
        <span>{{ errorMessage }}</span>
        <div class="flex shrink-0 gap-3">
          <button type="button" class="rounded underline underline-offset-2 focus-visible:outline-2" @click="retry">{{ t('internalBrowser.retry') }}</button>
          <button type="button" class="rounded underline underline-offset-2 focus-visible:outline-2" @click="runAction('external')">{{ t('internalBrowser.external') }}</button>
        </div>
      </div>
    </header>

    <section v-if="showDownloads" class="min-h-0 flex-1 overflow-auto p-5" :aria-label="t('internalBrowser.downloads')">
      <div class="mb-4 flex items-center justify-between"><h1 class="text-base font-semibold">{{ t('internalBrowser.downloads') }}</h1><button type="button" class="btn btn--secondary btn--sm" @click="toggleDownloads">{{ t('internalBrowser.returnToPage') }}</button></div>
      <p class="mb-5 text-[12px] text-text-secondary">{{ t('internalBrowser.downloadLocation') }}</p>
      <p v-if="!state.downloads.length" class="py-12 text-center text-[13px] text-text-secondary">{{ t('internalBrowser.noDownloads') }}</p>
      <ul class="space-y-2">
        <li v-for="item in [...state.downloads].reverse()" :key="item.id" class="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-border p-3">
          <div class="min-w-0"><div class="truncate text-[13px] font-medium" :title="item.name">{{ item.name }}</div><div class="mt-1 truncate text-[12px] text-text-secondary" :title="item.path || item.url">{{ item.path || item.url }}</div></div>
          <div class="flex shrink-0 items-center gap-3 text-[12px]"><span :class="item.status === 'failed' ? 'text-danger' : 'text-text-secondary'">{{ t(`internalBrowser.downloadStatus.${item.status}`) }}</span><button v-if="item.status === 'finished' && item.path" type="button" class="btn btn--secondary btn--sm" @click="runAction('reveal', item.id)">{{ t('internalBrowser.reveal') }}</button></div>
        </li>
      </ul>
    </section>
  </main>
</template>

<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getWebsiteUrl } from '../../utils/spotlightUrl'

const { t, locale } = useI18n()
const state = ref({ url: '', title: '', loading: true, error: null, downloads: [], revision: 0 })
const address = ref('')
const addressRef = ref(null)
const headerRef = ref(null)
const editingAddress = ref(false)
const ready = ref(false)
const showDownloads = ref(false)
const actionError = ref('')
const feedback = ref('')
const activeDownloads = computed(() => state.value.downloads.filter(item => item.status === 'downloading').length)
const errorMessage = computed(() => actionError.value || (state.value.error ? t(`internalBrowser.${state.value.error}`) : ''))
const navigationActions = [
  { id: 'back', path: 'm14 5-7 7 7 7' },
  { id: 'forward', path: 'm10 5 7 7-7 7' }
]
const utilityActions = [
  { id: 'copy', path: 'M9 9h11v12H9zM5 15H3V3h12v2' },
  { id: 'external', path: 'M14 3h7v7m0-7L10 14M10 3H3v18h18v-7' }
]
let unlisten
let unlistenFocus
let pollTimer
let feedbackTimer
let observer
let disposed = false
let polling = false
let headerHeight = 0

function applyState(snapshot) {
  if (disposed || !Number.isSafeInteger(snapshot.revision) || snapshot.revision <= state.value.revision) return
  const previous = new Map(state.value.downloads.map(item => [item.id, item.status]))
  const finished = snapshot.downloads.find(item => previous.get(item.id) === 'downloading' && item.status !== 'downloading')
  state.value = snapshot
  ready.value = true
  if (!editingAddress.value) address.value = snapshot.url
  if (finished) notify(t(finished.status === 'finished' ? 'internalBrowser.downloadComplete' : 'internalBrowser.downloadFailed'))
}

function notify(message) {
  feedback.value = message
  clearTimeout(feedbackTimer)
  feedbackTimer = setTimeout(() => { feedback.value = '' }, 4000)
}

async function refreshState() {
  if (polling || disposed) return
  polling = true
  try { applyState(await invoke('internal_browser_state')) }
  catch { if (!ready.value) actionError.value = t('internalBrowser.startFailed') }
  finally { polling = false }
}

async function runAction(action, value = null) {
  actionError.value = ''
  try {
    await invoke('internal_browser_action', { action, value })
    if (action === 'copy') notify(t('internalBrowser.copied'))
    await refreshState()
    return true
  } catch {
    actionError.value = t('internalBrowser.actionFailed')
    return false
  }
}

async function navigate() {
  const url = getWebsiteUrl(address.value)
  if (!url) { actionError.value = t('internalBrowser.invalidUrl'); return }
  editingAddress.value = false
  if (showDownloads.value) await toggleDownloads()
  await runAction('navigate', url)
  addressRef.value?.blur()
}

function restoreAddress() { address.value = state.value.url; addressRef.value?.blur() }
async function retry() {
  if (!ready.value) { actionError.value = ''; await refreshState(); return }
  await runAction('navigate', state.value.url)
}

async function toggleDownloads() {
  const next = !showDownloads.value
  if (await runAction('downloads', next ? 'open' : 'close')) showDownloads.value = next
}

function handleKeydown(event) {
  if (event.isComposing || event.keyCode === 229) return
  const modifier = event.ctrlKey || event.metaKey
  if (modifier && event.key.toLowerCase() === 'l') {
    event.preventDefault(); addressRef.value?.focus(); addressRef.value?.select()
  } else if ((modifier && event.key.toLowerCase() === 'r') || event.key === 'F5') {
    event.preventDefault(); runAction('reload')
  } else if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
    event.preventDefault(); runAction(event.key === 'ArrowLeft' ? 'back' : 'forward')
  }
}

onMounted(async () => {
  const language = localStorage.getItem('preferred-language')
  if (['zh-CN', 'en-US'].includes(language)) locale.value = language
  const font = localStorage.getItem('user-font-sans')
  if (font) document.documentElement.style.setProperty('--font-sans', `"${font}", system-ui, sans-serif`)
  try {
    const cleanup = await listen('internal-browser-state', event => applyState(event.payload))
    if (disposed) { cleanup(); return }
    unlisten = cleanup
    const cleanupFocus = await listen('internal-browser-focus-address', () => { addressRef.value?.focus(); addressRef.value?.select() })
    if (disposed) { cleanupFocus(); return }
    unlistenFocus = cleanupFocus
  } catch { /* Polling also recovers missed native events. */ }
  await refreshState()
  if (disposed) return
  pollTimer = setInterval(refreshState, 1000)
  await nextTick()
  observer = new ResizeObserver(async () => {
    const height = Math.ceil(headerRef.value?.getBoundingClientRect().height || 55)
    if (height === headerHeight || disposed) return
    headerHeight = height
    try { await invoke('internal_browser_action', { action: 'layout', value: String(height) }) }
    catch { headerHeight = 0 }
  })
  if (headerRef.value) observer.observe(headerRef.value)
})

onUnmounted(() => {
  disposed = true
  unlisten?.()
  unlistenFocus?.()
  observer?.disconnect()
  clearInterval(pollTimer)
  clearTimeout(feedbackTimer)
})
</script>

<style scoped>
.browser-toolbar { display: flex; height: 52px; align-items: center; gap: 6px; padding: 0 12px; }
.browser-button { display: flex; width: 30px; height: 30px; flex-shrink: 0; align-items: center; justify-content: center; border-radius: 6px; color: var(--text-secondary); }
.browser-button:hover { background: var(--hover); color: var(--text); }
.browser-button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.browser-button:disabled { opacity: .4; cursor: default; }
.browser-button svg { width: 17px; height: 17px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
.browser-progress { animation: browser-loading 1.2s ease-in-out infinite; }
@keyframes browser-loading { from { transform: translateX(-100%); } to { transform: translateX(400%); } }
@media (prefers-reduced-motion: reduce) { .browser-progress { animation: none; width: 100%; opacity: .4; } }
@media (max-width: 480px) {
  .browser-toolbar { height: auto; min-height: 52px; flex-wrap: wrap; gap: 4px; padding: 8px; }
  .browser-address { order: 1; flex: 1 0 100%; margin: 0; }
}
</style>
