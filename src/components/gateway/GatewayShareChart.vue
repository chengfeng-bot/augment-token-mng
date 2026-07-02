<template>
  <div class="rounded-lg border border-border bg-muted/10 p-3 h-[240px] flex flex-col">
    <div class="flex justify-between items-center mb-2 shrink-0">
      <h4 class="text-[13px] font-semibold text-text-secondary m-0">{{ title }}</h4>
      <span v-if="displayItems.length" class="text-[11px] text-text-muted">
        {{ $t('gateway.overview.shareRequests') }} {{ formatNumber(totalRequests) }}
      </span>
    </div>

    <div v-if="loading && !items.length" class="flex-1 flex items-center justify-center text-text-muted">
      <span class="spinner spinner--sm"></span>
    </div>

    <div v-else-if="!displayItems.length" class="flex-1 flex flex-col items-center justify-center gap-2 text-text-muted">
      <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" class="opacity-50">
        <path d="M11 2v20c-5.07-.5-9-4.79-9-10s3.93-9.5 9-10zm2.03 0v8.99H22c-.47-4.74-4.24-8.52-8.97-8.99zm0 11.01V22c4.74-.47 8.5-4.25 8.97-8.99h-8.97z"/>
      </svg>
      <p class="m-0 text-[12px]">{{ $t('gateway.overview.chartEmpty') }}</p>
    </div>

    <div v-else class="flex-1 min-h-0 relative">
      <Doughnut :key="chartKey" :data="doughnutData" :options="chartOptions" />
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Doughnut } from 'vue-chartjs'
import { Chart as ChartJS, ArcElement, Tooltip, Legend } from 'chart.js'

ChartJS.register(ArcElement, Tooltip, Legend)

const props = defineProps({
  title: { type: String, default: '' },
  loading: { type: Boolean, default: false },
  items: { type: Array, default: () => [] } // [{ label, requests, tokens, cost }]
})

const { t } = useI18n()

const PALETTE = ['#4c6ef5', '#12b886', '#f59f00', '#f783ac', '#7048e8', '#15aabf', '#fa5252', '#82c91e']
const OTHER_COLOR = '#adb5bd'
const MAX_SLICES = 8

const chartKey = ref(0)
const currentTheme = ref('light')

const readTheme = () => {
  if (typeof document === 'undefined') return 'light'
  return document.documentElement.dataset.theme || document.documentElement.getAttribute('data-theme') || 'light'
}

const resolveCssVar = (name, fallback) => {
  if (typeof document === 'undefined') return fallback
  const value = getComputedStyle(document.documentElement).getPropertyValue(name)?.trim()
  return value || fallback
}

const themePalette = computed(() => {
  const isDark = currentTheme.value === 'dark'
  return {
    surface: resolveCssVar('--surface', isDark ? '#171717' : '#ffffff'),
    legendColor: resolveCssVar('--text-secondary', isDark ? '#a3a3a3' : '#525252'),
    tooltipBg: resolveCssVar('--surface-elevated', isDark ? '#1f1f1f' : '#ffffff'),
    tooltipTitle: resolveCssVar('--text', isDark ? '#fafafa' : '#171717'),
    tooltipBody: resolveCssVar('--text-secondary', isDark ? '#a3a3a3' : '#525252'),
    tooltipBorder: resolveCssVar('--border-strong', isDark ? '#404040' : '#d4d4d4')
  }
})

const formatNumber = (v) => {
  const n = Number(v || 0)
  if (n < 1000) return n.toLocaleString()
  if (n < 1000000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'K'
  if (n < 1000000000) return (n / 1000000).toFixed(2).replace(/\.00$/, '') + 'M'
  return (n / 1000000000).toFixed(2).replace(/\.00$/, '') + 'B'
}

const formatCost = (v) => {
  const n = Number(v || 0)
  if (!n) return '$0'
  return n >= 1 ? `$${n.toFixed(2)}` : `$${n.toFixed(4)}`
}

// 请求数降序取前 N，其余合并为「其他」
const displayItems = computed(() => {
  const arr = [...props.items].filter((it) => it.requests > 0).sort((a, b) => b.requests - a.requests)
  if (arr.length <= MAX_SLICES) return arr
  const head = arr.slice(0, MAX_SLICES)
  const rest = arr.slice(MAX_SLICES)
  const other = rest.reduce(
    (acc, it) => ({ requests: acc.requests + it.requests, tokens: acc.tokens + it.tokens, cost: acc.cost + it.cost }),
    { requests: 0, tokens: 0, cost: 0 }
  )
  return [...head, { label: t('gateway.overview.shareOthers'), _other: true, ...other }]
})

const totalRequests = computed(() => displayItems.value.reduce((s, it) => s + it.requests, 0))

const doughnutData = computed(() => ({
  labels: displayItems.value.map((it) => it.label),
  datasets: [
    {
      data: displayItems.value.map((it) => it.requests),
      backgroundColor: displayItems.value.map((it, i) => (it._other ? OTHER_COLOR : PALETTE[i % PALETTE.length])),
      borderColor: themePalette.value.surface,
      borderWidth: 2,
      hoverOffset: 4
    }
  ]
}))

const chartOptions = computed(() => {
  const palette = themePalette.value
  return {
    responsive: true,
    maintainAspectRatio: false,
    cutout: '60%',
    plugins: {
      legend: {
        position: 'right',
        labels: { color: palette.legendColor, font: { size: 10 }, boxWidth: 8, boxHeight: 8, padding: 8 }
      },
      tooltip: {
        backgroundColor: palette.tooltipBg,
        titleColor: palette.tooltipTitle,
        bodyColor: palette.tooltipBody,
        borderColor: palette.tooltipBorder,
        borderWidth: 1,
        padding: 10,
        callbacks: {
          label: (ctx) => {
            const it = displayItems.value[ctx.dataIndex]
            if (!it) return ''
            const pct = totalRequests.value ? ((it.requests / totalRequests.value) * 100).toFixed(1) : '0'
            return ` ${it.label}: ${it.requests.toLocaleString()} (${pct}%)`
          },
          afterLabel: (ctx) => {
            const it = displayItems.value[ctx.dataIndex]
            if (!it) return ''
            return [`${t('gateway.overview.shareTokens')}: ${formatNumber(it.tokens)}`, `${t('gateway.overview.shareCost')}: ${formatCost(it.cost)}`]
          }
        }
      }
    }
  }
})

const updateTheme = () => { currentTheme.value = readTheme() }

let observer
onMounted(() => {
  updateTheme()
  if (typeof document === 'undefined') return
  observer = new MutationObserver(updateTheme)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
})
onUnmounted(() => { observer?.disconnect() })

watch(currentTheme, () => { chartKey.value++ })
</script>
