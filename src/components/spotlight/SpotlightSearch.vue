<template>
  <div class="flex h-screen flex-col overflow-hidden bg-surface text-text" @keydown="handleKeydown">
    <!-- 搜索栏 -->
    <div :class="['flex h-14 shrink-0 items-center gap-2.5 px-4', showPanel ? 'border-b border-border' : '']">
      <svg class="shrink-0 text-text-muted" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        ref="searchInputRef"
        v-model="query"
        :placeholder="$t('spotlight.placeholder')"
        class="min-w-0 flex-1 border-none bg-transparent text-[16px] text-text outline-none placeholder:text-text-muted"
        type="text"
        autocomplete="off"
        spellcheck="false"
        role="combobox"
        :aria-label="$t('spotlight.placeholder')"
        aria-autocomplete="list"
        :aria-controls="showResults ? 'spotlight-results' : undefined"
        :aria-expanded="showResults"
        :aria-activedescendant="showResults ? `spotlight-result-${activeIndex}` : undefined"
      />
      <button v-if="query" type="button" class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-text-muted hover:bg-hover hover:text-text focus-visible:outline-2 focus-visible:outline-accent" :aria-label="$t('spotlight.clearInput')" :title="$t('spotlight.clearHint')" @click="clearQuery">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
          <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
        </svg>
      </button>
      <kbd class="shrink-0 rounded border border-border bg-muted px-1.5 py-0.5 font-sans text-[11px] text-text-muted">ESC</kbd>
    </div>

    <div v-show="showPanel" ref="resultsRef" class="min-h-0 flex-1 overflow-y-auto">
      <div ref="contentRef">
        <div v-if="openError" role="alert" class="mx-2 mt-2 flex items-center justify-between gap-3 rounded-lg bg-danger-muted px-3 py-2 text-[12px] text-danger">
          <span>{{ $t(openError.bookmark.resultType === 'website' ? 'spotlight.visitFailed' : 'spotlight.openFailed') }}</span>
          <button type="button" class="shrink-0 rounded px-2 py-1 font-medium hover:bg-danger-tech focus-visible:outline-2 focus-visible:outline-danger" :disabled="isOpening" @click="retryOpen">{{ $t('spotlight.retry') }}</button>
        </div>
        <div v-if="isLoading" role="status" class="flex h-28 items-center justify-center gap-2 text-[13px] text-text-secondary">
          <span class="h-4 w-4 animate-spin rounded-full border-2 border-border border-t-accent motion-reduce:animate-none" aria-hidden="true"></span>
          {{ $t('spotlight.loading') }}
        </div>
        <div v-else-if="loadError" role="alert" class="flex min-h-28 flex-col items-center justify-center gap-3 px-4 py-5 text-[13px] text-text-secondary">
          <span>{{ $t('spotlight.loadFailed') }}</span>
          <button type="button" class="rounded-md border border-border px-3 py-1 text-text hover:bg-hover focus-visible:outline-2 focus-visible:outline-accent" @click="loadBookmarks">{{ $t('spotlight.retry') }}</button>
        </div>
        <div v-else-if="!bookmarks.length && !showResults" role="status" class="flex min-h-28 flex-col items-center justify-center gap-1.5 px-4 py-5 text-[13px]">
          <span>{{ $t('spotlight.noBookmarks') }}</span>
          <span class="text-[12px] text-text-secondary">{{ $t('spotlight.noBookmarksHint') }}</span>
        </div>
        <div v-else-if="query.trim() && !showResults" role="status" class="flex min-h-28 flex-col items-center justify-center gap-1.5 px-4 py-5 text-[13px]">
          <span>{{ $t('spotlight.noResults') }}</span>
          <span class="text-[12px] text-text-secondary">{{ $t('spotlight.noResultsHint') }}</span>
        </div>
        <!-- 结果列表 -->
        <div v-if="showResults" id="spotlight-results" role="listbox" :aria-label="$t('spotlight.resultsLabel')" :aria-busy="isOpening" class="p-1.5">
          <div
            v-for="(bookmark, index) in searchResults"
            :key="bookmark.resultKey"
            :id="`spotlight-result-${index}`"
            role="option"
            :aria-selected="index === activeIndex"
            :class="['flex min-h-14 cursor-pointer items-center gap-2.5 rounded-lg border-l-2 px-2.5 py-2 transition-colors duration-75 motion-reduce:transition-none', index === activeIndex ? 'border-accent bg-accent-muted' : 'border-transparent hover:bg-hover']"
            @click="openBookmark(bookmark, $event)"
            @mousemove="activeIndex = index"
          >
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border bg-surface text-[14px] font-semibold text-text-secondary" aria-hidden="true">
              <svg v-if="bookmark.resultType === 'website'" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <circle cx="12" cy="12" r="9" />
                <ellipse cx="12" cy="12" rx="4" ry="9" />
                <path d="M3 12h18" />
              </svg>
              <template v-else>{{ bookmarkInitial(bookmark) }}</template>
            </div>
            <div class="min-w-0 flex-1">
              <div v-if="bookmark.resultType === 'website'" class="truncate text-[14px] font-medium leading-5 text-text" :title="$t('spotlight.visitAddress', { address: bookmark.name })">{{ $t('spotlight.visitAddress', { address: bookmark.name }) }}</div>
              <div v-else class="truncate text-[14px] font-medium leading-5 text-text" :title="bookmark.name || bookmark.url" v-html="highlightText(bookmark.name || bookmark.url)"></div>
              <div v-if="bookmark.url" class="mt-0.5 truncate text-[12px] leading-4 text-text-secondary" :title="bookmark.url" v-html="highlightText(bookmark.resultType === 'website' ? bookmark.url : displayUrl(bookmark.url))"></div>
              <div v-if="descriptionSnippet(bookmark)" class="mt-1 truncate text-[12px] leading-4 text-text-secondary" :title="bookmark.description" v-html="highlightText(descriptionSnippet(bookmark))"></div>
            </div>
            <span v-if="bookmark.resultType === 'website'" class="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[11px] text-text-secondary">{{ $t('spotlight.visitWebsite') }}</span>
            <div v-else-if="bookmark.tag" class="max-w-28 shrink-0 truncate rounded-full px-2 py-0.5 text-[11px] font-medium" :title="bookmark.tag" :style="{ background: `color-mix(in srgb, ${bookmark.tag_color || DEFAULT_TAG_COLOR} 15%, transparent)`, color: bookmark.tag_color || DEFAULT_TAG_COLOR }" v-html="highlightText(bookmark.tag)"></div>
            <kbd :class="['w-4 shrink-0 text-center font-sans text-[12px] text-text-secondary', index === activeIndex ? '' : 'invisible']" aria-hidden="true">↵</kbd>
          </div>
        </div>
      </div>
    </div>

    <!-- 底部状态栏 -->
    <div v-if="showPanel" class="flex h-9 shrink-0 items-center justify-between gap-3 border-t border-border px-4 text-[11px] text-text-secondary">
      <span class="min-w-0 truncate" role="status">{{ footerStatus }}</span>
      <div class="flex shrink-0 items-center gap-3">
        <span class="flex items-center gap-1">
          <kbd class="rounded border border-border bg-muted px-1 py-px font-sans text-[10px]">↑↓</kbd>
          {{ $t('spotlight.navigate') }}
        </span>
        <span class="flex items-center gap-1">
          <kbd class="rounded border border-border bg-muted px-1 py-px font-sans text-[10px]">↵</kbd>
          {{ $t('spotlight.open') }}
        </span>
        <span class="flex items-center gap-1">
          <kbd class="rounded border border-border bg-muted px-1 py-px font-sans text-[10px]">{{ isMac ? '⌘↵' : 'Ctrl↵' }}</kbd>
          {{ $t('spotlight.openInternal') }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { useI18n } from 'vue-i18n'
import { match as pinyinMatch } from 'pinyin-pro'
import { getWebsiteUrl } from '../../utils/spotlightUrl'

const { t: $t, locale } = useI18n()

const isMac = navigator.platform?.toLowerCase().includes('mac') || navigator.userAgent?.toLowerCase().includes('mac')
const DEFAULT_TAG_COLOR = '#f97316'
const WINDOW_WIDTH = 680
const SEARCH_BAR_HEIGHT = 56
const MAX_WINDOW_HEIGHT = 480
const FOOTER_HEIGHT = 36
const RESULT_LIMIT = 50

const query = ref('')
const bookmarks = ref([])
const activeIndex = ref(0)
const isLoading = ref(true)
const loadError = ref(false)
const openError = ref(null)
const isOpening = ref(false)
const searchInputRef = ref(null)
const resultsRef = ref(null)
const contentRef = ref(null)
const showPanel = computed(() => Boolean(query.value.trim() || isLoading.value || loadError.value || !bookmarks.value.length || openError.value))
const showResults = computed(() => searchResults.value.length > 0)
const footerStatus = computed(() => {
  if (isOpening.value) return $t('spotlight.opening')
  if (!query.value.trim() || ((isLoading.value || loadError.value) && !websiteResult.value)) return ''
  const count = (isLoading.value || loadError.value ? 0 : matchedBookmarks.value.length) + (websiteResult.value ? 1 : 0)
  return count > searchResults.value.length
    ? $t('spotlight.resultsLimited', { count, limit: searchResults.value.length })
    : $t('spotlight.resultsCount', { count })
})

// 文本匹配检查（直接 + 拼音）
const textMatch = (target, q) => {
  if (!target) return false
  if (target.toLowerCase().includes(q)) return true
  try {
    const result = pinyinMatch(target, q)
    return result !== null && result !== undefined
  } catch {
    return false
  }
}

// 相关性评分
const scoreBookmark = (b, q) => {
  let score = 0
  const name = (b.name || '').toLowerCase()
  const url = (b.url || '').toLowerCase()
  const tag = (b.tag || '').toLowerCase()
  const desc = (b.description || '').toLowerCase()

  // 名称精确匹配（最高优先级）
  if (name === q) score += 1000
  // 名称以查询开头
  else if (name.startsWith(q)) score += 800
  // 名称包含查询
  else if (name.includes(q)) score += 600

  // 标签包含
  if (tag.includes(q)) score += 400

  // URL 包含
  if (url.includes(q)) score += 200

  // 描述包含
  if (desc.includes(q)) score += 100

  // 拼音匹配（最低优先级，仅在直接文本未匹配时补充）
  if (score === 0) {
    try {
      if (pinyinMatch(b.name, q)) score += 50
      if (pinyinMatch(b.tag, q)) score += 30
      if (pinyinMatch(b.url, q)) score += 10
      if (pinyinMatch(b.description, q)) score += 5
    } catch {}
  }

  return score
}

// 搜索过滤 + 相关性排序
const matchedBookmarks = computed(() => {
  const q = query.value.toLowerCase().trim()
  if (!q) return []
  return bookmarks.value
    .map(b => ({ bookmark: b, score: scoreBookmark(b, q) }))
    .filter(item => item.score > 0)
    .sort((a, b) => b.score - a.score)
    .map(item => item.bookmark)
})
const filteredBookmarks = computed(() => matchedBookmarks.value.slice(0, RESULT_LIMIT))
const websiteResult = computed(() => {
  const url = getWebsiteUrl(query.value)
  return url ? { resultKey: 'website', resultType: 'website', name: new URL(url).host, url } : null
})
const searchResults = computed(() => {
  const results = isLoading.value || loadError.value ? [] : filteredBookmarks.value.map(bookmark => ({
    ...bookmark, resultKey: `bookmark:${bookmark.id}`, resultType: 'bookmark'
  }))
  return websiteResult.value ? [websiteResult.value, ...results] : results
})

const displayUrl = (url) => url.replace(/^https?:\/\//i, '').replace(/\/$/, '')
const bookmarkInitial = (bookmark) => {
  const label = bookmark.name?.trim() || displayUrl(bookmark.url || '')
  return [...label][0]?.toUpperCase() || '↗'
}

const descriptionSnippet = (bookmark) => {
  const text = bookmark.description
  const q = query.value.trim().toLowerCase()
  if (!q || !textMatch(text, q)) return ''
  let index = text.toLowerCase().indexOf(q)
  if (index < 0) {
    try { index = pinyinMatch(text, q)?.[0] ?? 0 } catch { index = 0 }
  }
  const start = Math.max(0, index - 18)
  const end = Math.min(text.length, Math.max(index + q.length + 36, start + 80))
  return `${start > 0 ? '…' : ''}${text.slice(start, end)}${end < text.length ? '…' : ''}`
}

// 搜索关键词高亮
const highlightText = (text) => {
  if (!text) return ''
  const q = query.value.trim()
  if (!q) return escapeHtml(text)

  // 先尝试直接文本匹配高亮
  const escaped = escapeRegExp(q)
  const regex = new RegExp(`(${escaped})`, 'gi')
  if (regex.test(text)) {
    return text.split(regex).map((part, index) => index % 2
      ? `<span class="text-accent font-semibold">${escapeHtml(part)}</span>`
      : escapeHtml(part)).join('')
  }

  // 拼音匹配高亮
  try {
    const indices = pinyinMatch(text, q)
    if (indices && indices.length > 0) {
      const chars = [...text]
      const indexSet = new Set(indices)
      return chars.map((ch, i) =>
        indexSet.has(i)
          ? `<span class="text-accent font-semibold">${escapeHtml(ch)}</span>`
          : escapeHtml(ch)
      ).join('')
    }
  } catch {}

  return escapeHtml(text)
}

const escapeHtml = (str) => {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

const escapeRegExp = (str) => {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

// 动态调整窗口高度
let requestedHeight = SEARCH_BAR_HEIGHT
let appliedHeight = null
let resizeTask = Promise.resolve()
let disposed = false
const resizeWindow = async () => {
  requestedHeight = showPanel.value
    ? Math.min(MAX_WINDOW_HEIGHT, SEARCH_BAR_HEIGHT + FOOTER_HEIGHT + Math.ceil(contentRef.value?.getBoundingClientRect().height || 112))
    : SEARCH_BAR_HEIGHT
  if (disposed) return
  resizeTask = resizeTask.then(async () => {
    while (!disposed && appliedHeight !== requestedHeight) {
      const height = requestedHeight
      await getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, height))
      appliedHeight = height
    }
  }).catch(e => {
    console.error('Failed to resize spotlight:', e)
  })
  await resizeTask
}

watch(query, () => {
  activeIndex.value = 0
  openError.value = null
  nextTick(() => { if (resultsRef.value) resultsRef.value.scrollTop = 0 })
})

watch([showPanel, searchResults, isLoading, loadError, openError], resizeWindow, { flush: 'post' })
watch(searchResults, () => { activeIndex.value = 0 })
watch(activeIndex, () => {
  resultsRef.value?.querySelector(`[id="spotlight-result-${activeIndex.value}"]`)?.scrollIntoView({ block: 'nearest' })
}, { flush: 'post' })

const hideSpotlight = async () => {
  focusRequest += 1
  try {
    await invoke('hide_spotlight')
  } catch (e) {
    console.error('Failed to hide spotlight:', e)
  }
}

let loadVersion = 0
const loadBookmarks = async () => {
  const version = ++loadVersion
  isLoading.value = true
  loadError.value = false
  try {
    const result = await invoke('bookmark_load_local')
    if (version !== loadVersion || disposed) return
    bookmarks.value = result.bookmarks || []
  } catch (e) {
    if (version !== loadVersion || disposed) return
    loadError.value = true
    console.error('Failed to load bookmarks:', e)
  } finally {
    if (version === loadVersion && !disposed) isLoading.value = false
  }
}

// 打开搜索结果 —— 支持外部和内部浏览器
const openBookmark = async (bookmark, event) => {
  if (isOpening.value || (bookmark.resultType !== 'website' && (isLoading.value || loadError.value))) return
  const useInternal = Boolean(event?.ctrlKey || event?.metaKey)
  if (!bookmark.url) {
    openError.value = { bookmark, useInternal }
    return
  }
  isOpening.value = true
  openError.value = null
  try {
    if (useInternal) {
      await invoke('open_internal_browser', { url: bookmark.url, title: bookmark.name || bookmark.url })
    } else {
      await invoke('open_url', { url: bookmark.url })
    }
    await hideSpotlight()
  } catch (e) {
    openError.value = { bookmark, useInternal }
    console.error('Failed to open URL:', e)
  } finally {
    isOpening.value = false
  }
}

const retryOpen = () => {
  const failed = openError.value
  if (failed) openBookmark(failed.bookmark, { ctrlKey: failed.useInternal })
}

const clearQuery = () => {
  query.value = ''
  searchInputRef.value?.focus()
}

const handleKeydown = (e) => {
  if (e.isComposing || e.keyCode === 229) return
  if (e.target?.closest('button') && e.key !== 'Escape') return
  const list = searchResults.value
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      if (list.length > 0) {
        activeIndex.value = (activeIndex.value + 1) % list.length
      }
      break
    case 'ArrowUp':
      e.preventDefault()
      if (list.length > 0) {
        activeIndex.value = (activeIndex.value - 1 + list.length) % list.length
      }
      break
    case 'Enter':
      e.preventDefault()
      if (!e.repeat && list[activeIndex.value]) {
        openBookmark(list[activeIndex.value], e)
      }
      break
    case 'Escape':
      e.preventDefault()
      if (e.shiftKey && query.value) {
        clearQuery()
      } else {
        hideSpotlight()
      }
      break
  }
}

let unlistenFocus = null
let unlistenInputFocus = null
let contentObserver = null
let focusRequest = 0

const focusSearchInput = async () => {
  const request = ++focusRequest
  await nextTick()
  if (disposed || request !== focusRequest) return
  const input = searchInputRef.value
  if (input && document.activeElement !== input) {
    input.focus({ preventScroll: true })
  }
}

// 同步主题和语言：从 localStorage 读取并应用到当前窗口
const syncPreferences = () => {
  try {
    const theme = localStorage.getItem('atm-theme')
    if (theme === 'dark' || theme === 'light') {
      document.documentElement.dataset.theme = theme
      document.documentElement.style.colorScheme = theme
    }
  } catch {}
  try {
    const lang = localStorage.getItem('preferred-language')
    if (lang && (lang === 'zh-CN' || lang === 'en-US')) {
      locale.value = lang
    }
  } catch {}
  try {
    const savedFont = localStorage.getItem('user-font-sans')
    if (savedFont) {
      document.documentElement.style.setProperty('--font-sans', `"${savedFont}", system-ui, sans-serif`)
    } else {
      document.documentElement.style.removeProperty('--font-sans')
    }
  } catch {}
}

onMounted(async () => {
  syncPreferences()
  contentObserver = new ResizeObserver(resizeWindow)
  if (contentRef.value) contentObserver.observe(contentRef.value)
  // 监听后端发来的聚焦事件（窗口重新显示时）
  const unlisten = await listen('spotlight-focus', () => {
    syncPreferences()
    query.value = ''
    activeIndex.value = 0
    openError.value = null
    loadBookmarks()
    focusSearchInput()
  })
  if (disposed) { unlisten(); return }
  unlistenFocus = unlisten
  const unlistenInput = await listen('spotlight-input-focus', focusSearchInput)
  if (disposed) { unlistenInput(); return }
  unlistenInputFocus = unlistenInput

  await loadBookmarks()
  await nextTick()
  await resizeWindow()
  if (disposed) return
  const shown = await invoke('spotlight_ready')
  if (shown) await focusSearchInput()
})

onUnmounted(() => {
  disposed = true
  focusRequest += 1
  contentObserver?.disconnect()
  if (unlistenFocus) unlistenFocus()
  if (unlistenInputFocus) unlistenInputFocus()
})
</script>
