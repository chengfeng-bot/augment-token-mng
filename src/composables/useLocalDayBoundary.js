import { onActivated, onMounted, onUnmounted, readonly, ref } from 'vue'

const startOfLocalDay = (date = new Date()) =>
  new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()

const localDayStart = ref(startOfLocalDay())
let midnightTimer = null
let consumers = 0

const scheduleNextMidnight = () => {
  if (midnightTimer) window.clearTimeout(midnightTimer)
  const now = new Date()
  const nextMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime()
  midnightTimer = window.setTimeout(refreshLocalDay, Math.max(1000, nextMidnight - now.getTime() + 100))
}

const refreshLocalDay = () => {
  const next = startOfLocalDay()
  if (localDayStart.value !== next) localDayStart.value = next
  if (consumers > 0) scheduleNextMidnight()
}

const handleVisibilityChange = () => {
  if (!document.hidden) refreshLocalDay()
}

const startTracking = () => {
  consumers += 1
  if (consumers === 1) {
    document.addEventListener('visibilitychange', handleVisibilityChange)
    window.addEventListener('focus', refreshLocalDay)
  }
  refreshLocalDay()
}

const stopTracking = () => {
  consumers = Math.max(0, consumers - 1)
  if (consumers > 0) return
  if (midnightTimer) {
    window.clearTimeout(midnightTimer)
    midnightTimer = null
  }
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  window.removeEventListener('focus', refreshLocalDay)
}

export const useLocalDayBoundary = () => {
  onMounted(startTracking)
  onActivated(refreshLocalDay)
  onUnmounted(stopTracking)
  return readonly(localDayStart)
}
