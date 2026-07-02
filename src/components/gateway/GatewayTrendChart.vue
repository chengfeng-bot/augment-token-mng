<template>
  <div class="rounded-lg border border-border bg-muted/10 p-3 h-[280px] flex flex-col">
    <div class="flex justify-between items-center mb-2 shrink-0">
      <h4 class="text-[13px] font-semibold text-text-secondary m-0">{{ $t('gateway.overview.trendTitle') }}</h4>
      <div class="flex items-center gap-3 text-[11px]">
        <button
          class="flex items-center gap-1 transition-opacity hover:opacity-70"
          :class="activeMetric === 'reqTokens' ? 'opacity-100' : 'opacity-40'"
          @click="activeMetric = 'reqTokens'"
        >
          <span class="flex items-center gap-1">
            <span class="w-2 h-2 rounded-full" :style="{ background: colors.requests }"></span>
            <span class="w-2 h-2 rounded-full" :style="{ background: colors.tokens }"></span>
          </span>
          {{ $t('gateway.overview.trendReqTokens') }}
        </button>
        <button
          class="flex items-center gap-1 transition-opacity hover:opacity-70"
          :class="activeMetric === 'cost' ? 'opacity-100' : 'opacity-40'"
          @click="activeMetric = 'cost'"
        >
          <span class="w-2.5 h-2.5 rounded-full" :style="{ background: colors.cost }"></span>
          {{ $t('gateway.overview.trendCost') }}
        </button>
      </div>
    </div>

    <div v-if="loading && !chartData.length" class="flex-1 flex flex-col items-center justify-center gap-2 text-text-muted">
      <span class="spinner spinner--sm"></span>
    </div>

    <div v-else-if="!chartData.length" class="flex-1 flex flex-col items-center justify-center gap-2 text-text-muted">
      <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" class="opacity-50">
        <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM9 17H7v-7h2v7zm4 0h-2V7h2v10zm4 0h-2v-4h2v4z"/>
      </svg>
      <p class="m-0 text-[12px]">{{ $t('gateway.overview.chartEmpty') }}</p>
    </div>

    <div v-else class="flex-1 min-h-0 relative">
      <Line :key="chartKey" :data="lineChartData" :options="chartOptions" />
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Line } from 'vue-chartjs'
import { Chart as ChartJS, CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Legend } from 'chart.js'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Legend)

const props = defineProps({
  loading: { type: Boolean, default: false },
  chartData: { type: Array, default: () => [] }
})

const { t } = useI18n()

const colors = { requests: '#4c6ef5', tokens: '#f783ac', cost: '#12b886' }

const activeMetric = ref('reqTokens') // 'reqTokens' | 'cost'
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
    gridColor: resolveCssVar('--border', isDark ? '#404040' : '#e5e5e5'),
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
  if (n < 1000000000000) return (n / 1000000000).toFixed(2).replace(/\.00$/, '') + 'B'
  return (n / 1000000000000).toFixed(2).replace(/\.00$/, '') + 'T'
}

const formatCost = (v) => {
  const n = Number(v || 0)
  if (!n) return '$0'
  return n >= 1 ? `$${n.toFixed(2)}` : `$${n.toFixed(4)}`
}

const formatLabel = (dateStr) => {
  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return dateStr
  return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

const lineStyle = { borderWidth: 2, tension: 0.3, pointRadius: 2, pointHoverRadius: 5, fill: false }

const lineChartData = computed(() => {
  if (!props.chartData.length) return { labels: [], datasets: [] }
  const labels = props.chartData.map((d) => formatLabel(d.date))

  if (activeMetric.value === 'cost') {
    return {
      labels,
      datasets: [{ label: 'cost', data: props.chartData.map((d) => d.cost || 0), yAxisID: 'yLeft', borderColor: colors.cost, backgroundColor: colors.cost, ...lineStyle }]
    }
  }

  return {
    labels,
    datasets: [
      { label: 'requests', data: props.chartData.map((d) => d.requests || 0), yAxisID: 'yLeft', borderColor: colors.requests, backgroundColor: colors.requests, ...lineStyle },
      { label: 'tokens', data: props.chartData.map((d) => d.tokens || 0), yAxisID: 'yRight', borderColor: colors.tokens, backgroundColor: colors.tokens, ...lineStyle }
    ]
  }
})

const leftFormatter = computed(() => (activeMetric.value === 'cost' ? formatCost : formatNumber))

const chartOptions = computed(() => {
  const palette = themePalette.value
  const metric = activeMetric.value
  const fmtLeft = leftFormatter.value

  const baseAxis = (position, display, drawOnChartArea, formatter) => ({
    display,
    position,
    grid: { color: palette.gridColor, drawBorder: false, drawOnChartArea },
    ticks: {
      color: palette.legendColor,
      font: { size: 10 },
      callback: (value) => (value < 1 ? '' : formatter(value))
    },
    min: 0,
    beginAtZero: true
  })

  return {
    responsive: true,
    maintainAspectRatio: false,
    interaction: { mode: 'index', intersect: false },
    plugins: {
      legend: { display: false },
      tooltip: {
        backgroundColor: palette.tooltipBg,
        titleColor: palette.tooltipTitle,
        bodyColor: palette.tooltipBody,
        borderColor: palette.tooltipBorder,
        borderWidth: 1,
        padding: 10,
        displayColors: true,
        callbacks: {
          title: (items) => (items.length ? props.chartData[items[0].dataIndex]?.date || '' : ''),
          label: (context) => {
            const d = props.chartData[context.dataIndex]
            if (!d) return ''
            if (metric === 'cost') return `${t('gateway.overview.trendCost')}: ${formatCost(d.cost)}`
            if (context.dataset?.label === 'tokens') return `${t('gateway.usage.tokens')}: ${formatNumber(d.tokens)}`
            return `${t('gateway.usage.totalRequests')}: ${formatNumber(d.requests)}`
          }
        }
      }
    },
    scales: {
      x: {
        grid: { display: false },
        ticks: { color: palette.legendColor, font: { size: 10 }, maxRotation: 0, autoSkip: true, maxTicksLimit: 7 }
      },
      yLeft: baseAxis('left', true, true, fmtLeft),
      yRight: baseAxis('right', metric === 'reqTokens', false, formatNumber)
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
watch(activeMetric, () => { chartKey.value++ })
</script>
