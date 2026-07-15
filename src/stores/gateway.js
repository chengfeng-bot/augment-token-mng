import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// 网关前端 store。后端 Tauri 命令（task 7 实现）契约：
//   gateway_get_config / gateway_set_config / gateway_get_status
//   gateway_start / gateway_stop
//   gateway_list_usage / gateway_clear_usage
//   gateway_list_bindable_accounts
// 后端未就绪时，动作 try/catch 回退到 localStorage，便于前端先行联调。

const CONFIG_KEY = 'gateway-config'
const CUSTOM_MODELS_KEY = 'gateway-custom-models'
const DEFAULT_PORT = 8766
const CUSTOM_GROUP_ID = '__custom__'
const USAGE_EVENT_BATCH_MS = 200

const genId = () => `ch_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`

const loadLocalConfig = () => {
  try {
    const raw = localStorage.getItem(CONFIG_KEY)
    if (raw) return JSON.parse(raw)
  } catch (e) {
    console.warn('Failed to read local gateway config', e)
  }
  return null
}

const loadLocalCustomModels = () => {
  try {
    const raw = localStorage.getItem(CUSTOM_MODELS_KEY)
    if (raw) return JSON.parse(raw)
  } catch (e) {
    console.warn('Failed to read local custom models', e)
  }
  return []
}

export const useGatewayStore = defineStore('gateway', () => {
  // State
  const status = ref({ running: false, address: '', port: DEFAULT_PORT })
  const config = ref({ enabled: false, port: DEFAULT_PORT, apiKey: '' })
  const channels = ref([])
  const usage = ref([])
  const bindableAccounts = ref([]) // { id, label, platform }
  const codexAccounts = ref([]) // openai_list_accounts 完整账号（含 quota），供 Codex OAuth 渠道卡片展示配额
  const models = ref([]) // [{ id, name, models: [{ id, family?, release_date?, custom?, ... }] }]
  const syncedProviders = ref([]) // 同步目录原始分组（不含自定义），用于合并重建
  const customModels = ref([]) // [{ id, developer?, cost?: { input, output, cache_read, cache_write }, ... }]
  const modelsSyncedAt = ref(null)

  const isLoadingConfig = ref(false)
  const isLoadingStatus = ref(false)
  const isTogglingServer = ref(false)
  const isLoadingUsage = ref(false)
  const isSyncingModels = ref(false)
  const configLoaded = ref(false)
  const modelsLoaded = ref(false)

  let pendingUsageChanges = []
  let usageFlushTimer = null
  let usageChangeSequence = 0
  let latestUsageLoadId = 0
  let activeUsageLoads = 0
  const usageChangesDuringLoads = []

  const applyConfig = (data) => {
    if (!data) return
    config.value = {
      enabled: data.enabled || false,
      port: data.port || DEFAULT_PORT,
      apiKey: data.apiKey || ''
    }
    channels.value = Array.isArray(data.channels) ? data.channels : []
  }

  const snapshot = () => ({
    enabled: config.value.enabled,
    apiKey: config.value.apiKey,
    channels: channels.value
  })

  const applyUsageChanges = (records, changes) => {
    let next = Array.isArray(records) ? [...records] : []
    for (const change of changes) {
      if (change?.type === 'recorded' && change.record?.requestId) {
        const idx = next.findIndex((record) => record.requestId === change.record.requestId)
        if (idx >= 0) next.splice(idx, 1)
        next.push(change.record)
      } else if (change?.type === 'cleared') {
        next = []
      } else if (change?.type === 'channelRemoved' && change.channelId) {
        next = next.filter((record) => record.channelId !== change.channelId)
      }
    }
    return next.sort((a, b) => (Number(b.createdAt) || 0) - (Number(a.createdAt) || 0))
  }

  const flushUsageChanges = () => {
    usageFlushTimer = null
    if (!pendingUsageChanges.length) return
    const changes = pendingUsageChanges
    pendingUsageChanges = []
    usage.value = applyUsageChanges(usage.value, changes)
  }

  // 高频请求按小批次合并，避免每条 Tauri 事件都触发整页派生统计重算
  const queueUsageChange = (change) => {
    if (!change || !['recorded', 'cleared', 'channelRemoved'].includes(change.type)) return
    usageChangeSequence += 1
    if (activeUsageLoads > 0) {
      usageChangesDuringLoads.push({ sequence: usageChangeSequence, change })
    }
    pendingUsageChanges.push(change)
    if (!usageFlushTimer) {
      usageFlushTimer = setTimeout(flushUsageChanges, USAGE_EVENT_BATCH_MS)
    }
  }

  const setGatewayRunning = (running) => {
    const value = Boolean(running)
    status.value = { ...status.value, running: value }
    config.value = { ...config.value, enabled: value }
  }

  // Actions
  const loadConfig = async (force = false) => {
    if (!force && configLoaded.value) return config.value
    isLoadingConfig.value = true
    try {
      const data = await invoke('gateway_get_config')
      applyConfig(data)
      configLoaded.value = true
    } catch (error) {
      console.warn('gateway_get_config unavailable, using local cache:', error)
      applyConfig(loadLocalConfig())
      configLoaded.value = true
    } finally {
      isLoadingConfig.value = false
    }
    return config.value
  }

  const saveConfig = async () => {
    const payload = snapshot()
    try {
      localStorage.setItem(CONFIG_KEY, JSON.stringify(payload))
    } catch (e) {
      console.warn('Failed to cache gateway config', e)
    }
    try {
      await invoke('gateway_set_config', { config: payload })
    } catch (error) {
      console.warn('gateway_set_config unavailable, cached locally only:', error)
    }
  }

  const loadStatus = async (force = false) => {
    isLoadingStatus.value = true
    try {
      const s = await invoke('gateway_get_status')
      status.value = {
        running: s.running || false,
        address: s.address || '',
        port: s.port || config.value.port || DEFAULT_PORT
      }
      config.value = { ...config.value, enabled: Boolean(s.running) }
    } catch (error) {
      console.warn('gateway_get_status unavailable:', error)
      status.value = { running: false, address: '', port: config.value.port || DEFAULT_PORT }
    } finally {
      isLoadingStatus.value = false
    }
    return status.value
  }

  const startServer = async () => {
    isTogglingServer.value = true
    try {
      await invoke('gateway_start')
    } catch (error) {
      console.warn('gateway_start unavailable:', error)
      throw error
    } finally {
      isTogglingServer.value = false
      await loadStatus(true)
    }
  }

  const stopServer = async () => {
    isTogglingServer.value = true
    try {
      await invoke('gateway_stop')
    } catch (error) {
      console.warn('gateway_stop unavailable:', error)
      throw error
    } finally {
      isTogglingServer.value = false
      await loadStatus(true)
    }
  }

  const loadUsage = async () => {
    const loadId = ++latestUsageLoadId
    const startSequence = usageChangeSequence
    activeUsageLoads += 1
    isLoadingUsage.value = true
    try {
      const list = await invoke('gateway_list_usage')
      if (loadId === latestUsageLoadId) {
        const changes = usageChangesDuringLoads
          .filter((entry) => entry.sequence > startSequence)
          .map((entry) => entry.change)
        usage.value = applyUsageChanges(Array.isArray(list) ? list : [], changes)
      }
    } catch (error) {
      console.warn('gateway_list_usage unavailable:', error)
    } finally {
      activeUsageLoads -= 1
      if (activeUsageLoads === 0) {
        usageChangesDuringLoads.length = 0
        isLoadingUsage.value = false
      }
    }
    return usage.value
  }

  const clearUsage = async () => {
    try {
      await invoke('gateway_clear_usage')
      usage.value = []
    } catch (error) {
      console.warn('gateway_clear_usage unavailable:', error)
      throw error
    }
  }

  const loadBindableAccounts = async () => {
    try {
      const list = await invoke('gateway_list_bindable_accounts')
      bindableAccounts.value = Array.isArray(list) ? list : []
    } catch (error) {
      console.warn('gateway_list_bindable_accounts unavailable:', error)
      bindableAccounts.value = []
    }
    return bindableAccounts.value
  }

  // 加载完整 OpenAI 账号（含配额），供 Codex OAuth 渠道卡片展示 5h/7d 配额、订阅计划与重置额度
  const loadCodexAccounts = async () => {
    try {
      const list = await invoke('openai_list_accounts')
      codexAccounts.value = Array.isArray(list) ? list : []
    } catch (error) {
      console.warn('openai_list_accounts unavailable:', error)
      codexAccounts.value = []
    }
    return codexAccounts.value
  }

  // 单条更新 Codex 账号（刷新配额后写回，避免整表重载）
  const upsertCodexAccount = (account) => {
    if (!account?.id) return
    const idx = codexAccounts.value.findIndex((a) => a.id === account.id)
    if (idx >= 0) {
      const next = [...codexAccounts.value]
      next[idx] = account
      codexAccounts.value = next
    } else {
      codexAccounts.value = [...codexAccounts.value, account]
    }
  }

  // 合并同步目录与自定义模型为展示分组：自定义同 id 覆盖同步项，未匹配开发商归入「自定义」分组
  const rebuildModels = () => {
    const customIds = new Set(customModels.value.map((m) => m?.id).filter(Boolean))
    const groups = syncedProviders.value.map((p) => ({
      ...p,
      models: (Array.isArray(p.models) ? p.models : []).filter((m) => !customIds.has(m?.id))
    }))
    const groupById = new Map(groups.map((g) => [g.id, g]))
    for (const cm of customModels.value) {
      if (!cm?.id) continue
      const dev = (cm.developer || '').trim()
      const groupId = dev || CUSTOM_GROUP_ID
      let group = groupById.get(groupId)
      if (!group) {
        group = { id: groupId, name: dev || 'Custom', models: [] }
        groupById.set(groupId, group)
        groups.push(group)
      }
      const entry = { ...cm, custom: true }
      const idx = group.models.findIndex((m) => m?.id === cm.id)
      if (idx >= 0) group.models[idx] = entry
      else group.models.push(entry)
    }
    models.value = groups
  }

  // 模型库：本地读取与联网同步
  const applyModels = (data) => {
    syncedProviders.value = Array.isArray(data?.providers) ? data.providers : []
    modelsSyncedAt.value = data?.synced_at ?? null
    rebuildModels()
  }

  const loadCustomModels = async () => {
    try {
      const list = await invoke('gateway_get_custom_models')
      customModels.value = Array.isArray(list) ? list : []
    } catch (error) {
      console.warn('gateway_get_custom_models unavailable, using local cache:', error)
      customModels.value = loadLocalCustomModels()
    }
    rebuildModels()
    return customModels.value
  }

  const saveCustomModels = async () => {
    try {
      localStorage.setItem(CUSTOM_MODELS_KEY, JSON.stringify(customModels.value))
    } catch (e) {
      console.warn('Failed to cache custom models', e)
    }
    try {
      await invoke('gateway_set_custom_models', { models: customModels.value })
    } catch (error) {
      console.warn('gateway_set_custom_models unavailable, cached locally only:', error)
    }
  }

  const upsertCustomModel = async (model, originalId) => {
    const next = { ...model }
    const key = originalId ?? next.id
    const idx = customModels.value.findIndex((m) => m.id === key)
    if (idx >= 0) {
      customModels.value = customModels.value.map((m) => (m.id === key ? next : m))
    } else {
      customModels.value = [...customModels.value, next]
    }
    rebuildModels()
    await saveCustomModels()
    return next
  }

  const removeCustomModel = async (id) => {
    customModels.value = customModels.value.filter((m) => m.id !== id)
    rebuildModels()
    await saveCustomModels()
  }

  const loadModels = async (force = false) => {
    if (!force && modelsLoaded.value) return models.value
    await loadCustomModels()
    try {
      const data = await invoke('gateway_get_models')
      applyModels(data)
    } catch (error) {
      console.warn('gateway_get_models unavailable:', error)
      applyModels(null)
    } finally {
      modelsLoaded.value = true
    }
    return models.value
  }

  const syncModels = async () => {
    isSyncingModels.value = true
    try {
      await invoke('gateway_sync_models')
      await loadModels(true)
    } catch (error) {
      console.warn('gateway_sync_models failed:', error)
      throw error
    } finally {
      isSyncingModels.value = false
    }
    return models.value
  }

  // 渠道连通性：拉取上游模型列表 / 测试可用性（无状态，结果交由调用方处理）
  const fetchChannelModels = ({ kind, baseUrl, apiKey }) =>
    invoke('gateway_fetch_channel_models', { kind, baseUrl, apiKey })

  const testChannel = ({ kind, baseUrl, apiKey, wire, model }) =>
    invoke('gateway_test_channel', { kind, baseUrl, apiKey, wire, model })

  // 扁平化的全部模型 ID（用于渠道模型输入框的补全建议，已含自定义模型）
  const allModelIds = () => {
    const ids = new Set()
    for (const provider of models.value) {
      for (const model of provider.models || []) {
        if (model?.id) ids.add(model.id)
      }
    }
    return Array.from(ids)
  }

  // 渠道的本地增删改（保存即持久化）
  const upsertChannel = async (channel) => {
    const next = { ...channel }
    if (!next.id) {
      next.id = genId()
      channels.value = [...channels.value, next]
    } else {
      channels.value = channels.value.map((c) => (c.id === next.id ? next : c))
    }
    await saveConfig()
    return next
  }

  const removeChannel = async (id) => {
    channels.value = channels.value.filter((c) => c.id !== id)
    await saveConfig()
    usage.value = usage.value.filter((u) => u.channelId !== id)
  }

  return {
    status, config, channels, usage, bindableAccounts, codexAccounts, models, customModels, modelsSyncedAt,
    isLoadingConfig, isLoadingStatus, isTogglingServer, isLoadingUsage, isSyncingModels,
    loadConfig, saveConfig, loadStatus, startServer, stopServer, setGatewayRunning,
    loadUsage, clearUsage, loadBindableAccounts, loadCodexAccounts, upsertCodexAccount,
    queueUsageChange,
    loadModels, syncModels, allModelIds,
    loadCustomModels, upsertCustomModel, removeCustomModel,
    fetchChannelModels, testChannel,
    upsertChannel, removeChannel
  }
})
