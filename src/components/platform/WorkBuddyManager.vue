<template>
  <div class="flex h-full min-h-0 flex-col bg-page text-text">
    <div class="flex items-center justify-between border-b border-border bg-surface px-5 py-3">
      <div class="flex items-center gap-3">
        <img src="/icons/workbuddy.svg" alt="WorkBuddy" class="h-9 w-9 object-contain" />
        <div>
          <h2 class="text-lg font-semibold">WorkBuddy 自定义渠道</h2>
          <p class="text-xs text-text-muted">每个模型保留自己的渠道地址与 API Key，最终写入 models.json</p>
        </div>
      </div>
      <div class="flex items-center gap-2">
        <span class="rounded-full bg-accent/10 px-3 py-1 text-xs text-accent">已激活 {{ selectedModels.length }} 个模型</span>
        <button class="btn btn--primary" :disabled="saving || loading" @click="saveModels">
          {{ saving ? '保存中…' : '保存到 WorkBuddy' }}
        </button>
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-auto p-5">
      <div class="mx-auto grid max-w-6xl gap-5 lg:grid-cols-[320px_minmax(0,1fr)]">
        <section class="card h-fit p-4">
          <div class="mb-3 flex items-center justify-between">
            <div>
              <h3 class="font-semibold">渠道</h3>
              <p class="text-xs text-text-muted">{{ channels.length }} 个渠道 · 点击切换预览</p>
            </div>
            <button class="btn btn--secondary btn--sm" @click="startNewChannel">+ 添加渠道</button>
          </div>
          <div v-if="!channels.length" class="rounded-lg border border-dashed border-border p-4 text-center text-sm text-text-muted">
            还没有渠道。可以手动添加，或用右侧“探测模型”。
          </div>
          <button
            v-for="channel in channels"
            :key="channel.id"
            class="mb-2 w-full rounded-xl border p-3 text-left transition"
            :class="selectedChannelId === channel.id ? 'border-accent bg-accent/10' : 'border-border hover:border-accent/50'"
            @click="selectChannel(channel)"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="truncate font-medium">{{ channel.label }}</span>
              <span class="shrink-0 text-xs text-text-muted">{{ channel.models.length }} 个模型</span>
            </div>
            <div class="mt-1 truncate text-xs text-text-muted">{{ channel.url || '未设置地址' }}</div>
            <div class="mt-2 text-xs" :class="channelActiveCount(channel) ? 'text-accent' : 'text-text-muted'">
              {{ channelActiveCount(channel) }} 个已激活
            </div>
          </button>
        </section>

        <section class="space-y-5">
          <div class="card p-4">
            <div class="mb-3 flex items-center justify-between">
              <div>
                <h3 class="font-semibold">{{ editingChannel ? '编辑渠道' : '添加自定义渠道' }}</h3>
                <p class="text-xs text-text-muted">API Key 只用于请求探测与写入模型配置，不会作为独立账号保存。</p>
              </div>
              <button v-if="editingChannel" class="btn btn--ghost btn--sm" @click="startNewChannel">新建渠道</button>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              <label class="form-group md:col-span-2">
                <span class="label">渠道名称</span>
                <input v-model="draft.label" class="input" placeholder="例如：OpenRouter / Benma" />
              </label>
              <label class="form-group md:col-span-2">
                <span class="label">模型探测地址</span>
                <input v-model="draft.url" class="input" placeholder="https://api.example.com/v1 或完整 /models 地址" />
              </label>
              <label class="form-group md:col-span-2">
                <span class="label">API Key</span>
                <input v-model="draft.apiKey" class="input" type="password" placeholder="sk-…" autocomplete="off" />
              </label>
              <label class="form-group md:col-span-2">
                <span class="label">手动添加模型 ID</span>
                <div class="flex gap-2">
                  <input v-model="manualModelId" class="input" placeholder="例如：gpt-5.6-sol" @keyup.enter="addManualModel" />
                  <button class="btn btn--secondary shrink-0" :disabled="!manualModelId.trim()" @click="addManualModel">添加模型</button>
                </div>
              </label>
            </div>
            <div class="mt-3 flex flex-wrap gap-2">
              <button class="btn btn--secondary" :disabled="probing || !draft.url" @click="probeModels">
                {{ probing ? '探测中…' : '探测模型列表' }}
              </button>
              <button class="btn btn--primary" :disabled="!draft.url" @click="commitChannel">{{ editingChannel ? '更新渠道' : '加入渠道' }}</button>
              <button v-if="editingChannel" class="btn btn--ghost text-danger" @click="removeChannel">删除渠道</button>
            </div>
            <p v-if="error" class="mt-3 rounded-lg bg-danger/10 p-3 text-sm text-danger">{{ error }}</p>
          </div>

          <div class="card p-4">
            <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 class="font-semibold">模型预览与激活组合</h3>
                <p class="text-xs text-text-muted">可跨渠道勾选；同一模型 ID 在 WorkBuddy 中只能保留一份，后勾选的渠道会覆盖先前渠道。</p>
              </div>
              <div class="flex gap-2">
                <button class="btn btn--secondary btn--sm" :disabled="!selectedChannel" @click="selectChannelModels(true)">全选本渠道</button>
                <button class="btn btn--ghost btn--sm" :disabled="!selectedChannel" @click="selectChannelModels(false)">清空本渠道</button>
              </div>
            </div>
            <div v-if="!selectedChannel" class="rounded-lg border border-dashed border-border p-8 text-center text-sm text-text-muted">先添加或选择一个渠道</div>
            <div v-else-if="!selectedChannel.models.length" class="rounded-lg border border-dashed border-border p-8 text-center text-sm text-text-muted">该渠道还没有模型，请先探测或手动添加。</div>
            <div v-else class="grid gap-2 sm:grid-cols-2">
              <label v-for="model in selectedChannel.models" :key="modelKey(selectedChannel, model)" class="flex cursor-pointer items-start gap-3 rounded-lg border border-border p-3 hover:border-accent/60">
                <input class="mt-1" type="checkbox" :checked="isSelected(selectedChannel, model)" @change="toggleModel(selectedChannel, model)" />
                <span class="min-w-0">
                  <span class="block truncate text-sm font-medium">{{ model.name || model.id }}</span>
                  <span class="block truncate text-xs text-text-muted">{{ model.id }}</span>
                  <span v-if="duplicateModelId(model.id)" class="mt-1 block text-[11px] text-warning">该 ID 在其他渠道也存在</span>
                </span>
              </label>
            </div>
          </div>

          <div class="card p-4">
            <h3 class="font-semibold">当前已激活模型</h3>
            <p class="mb-3 text-xs text-text-muted">保存时只会写入这里的模型，适合组合“渠道一前 5 个 + 渠道二后 5 个”。</p>
            <div v-if="!selectedModels.length" class="text-sm text-text-muted">尚未选择模型。</div>
            <div v-else class="flex flex-wrap gap-2">
              <span v-for="model in selectedModels" :key="model.key" class="rounded-full border border-accent/30 bg-accent/10 px-3 py-1 text-xs text-accent">{{ model.model.name || model.model.id }} · {{ model.channel.label }}</span>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const loading = ref(true)
const saving = ref(false)
const probing = ref(false)
const error = ref('')
const channels = ref([])
const selectedChannelId = ref(null)
const selectedKeys = ref(new Set())
const draft = ref({ label: '', url: '', apiKey: '', models: [] })
const manualModelId = ref('')

const CHANNEL_LABELS_KEY = 'atm-workbuddy-channel-labels'
const DEFAULT_REASONING = Object.freeze({
  defaultEffort: 'high',
  supportedEfforts: ['high', 'low', 'medium', 'xhigh', 'max'],
  canDisableThinking: false
})
const withWorkBuddyDefaults = (model, channel = {}) => ({
  ...model,
  id: model.id,
  name: model.name || model.id,
  vendor: model.vendor || 'Custom',
  url: model.url || channel.url || '',
  apiKey: model.apiKey ?? channel.apiKey ?? '',
  supportsImages: model.supportsImages ?? true,
  supportsReasoning: model.supportsReasoning ?? true,
  supportsToolCall: model.supportsToolCall ?? true,
  useCustomProtocol: model.useCustomProtocol ?? false,
  reasoning: {
    ...DEFAULT_REASONING,
    ...(model.reasoning || {}),
    supportedEfforts: Array.isArray(model.reasoning?.supportedEfforts) && model.reasoning.supportedEfforts.length
      ? model.reasoning.supportedEfforts
      : [...DEFAULT_REASONING.supportedEfforts]
  },
  onlyReasoning: model.onlyReasoning ?? true
})
const keyForChannel = (url, apiKey) => {
  const source = `${String(url || '').trim().replace(/\/$/, '')}\u0000${String(apiKey || '').trim()}`
  let hash = 2166136261
  for (let index = 0; index < source.length; index += 1) {
    hash ^= source.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return `channel-${(hash >>> 0).toString(36)}`
}
const loadChannelLabels = () => {
  try { return JSON.parse(localStorage.getItem(CHANNEL_LABELS_KEY) || '{}') }
  catch { return {} }
}
const saveChannelLabels = () => {
  const labels = Object.fromEntries(channels.value.map((channel) => [channel.id, channel.label]))
  localStorage.setItem(CHANNEL_LABELS_KEY, JSON.stringify(labels))
}
const modelKey = (channel, model) => `${channel.id}\u0000${model.id}`
const selectedChannel = computed(() => channels.value.find((item) => item.id === selectedChannelId.value) || null)
const editingChannel = computed(() => Boolean(selectedChannel.value))

const selectedModels = computed(() => {
  const result = []
  channels.value.forEach((channel) => channel.models.forEach((model) => {
    if (selectedKeys.value.has(modelKey(channel, model))) result.push({ key: modelKey(channel, model), channel, model })
  }))
  return result
})

const channelActiveCount = (channel) => channel.models.filter((model) => selectedKeys.value.has(modelKey(channel, model))).length
const isSelected = (channel, model) => selectedKeys.value.has(modelKey(channel, model))
const duplicateModelId = (id) => channels.value.filter((channel) => channel.models.some((model) => model.id === id)).length > 1

const startNewChannel = () => {
  selectedChannelId.value = null
  draft.value = { label: '', url: '', apiKey: '', models: [] }
  manualModelId.value = ''
  error.value = ''
}

const selectChannel = (channel) => {
  selectedChannelId.value = channel.id
  syncDraft(channel)
  error.value = ''
}

const loadConfig = async () => {
  loading.value = true
  try {
    const result = await invoke('workbuddy_get_config')
    const groups = new Map()
    const labels = loadChannelLabels()
    ;(result?.models || []).forEach((model) => {
      const url = model.url || ''
      const apiKey = model.apiKey || ''
      const id = keyForChannel(url, apiKey)
      if (!groups.has(id)) groups.set(id, { id, label: labels[id] || (model.vendor && model.vendor !== 'Custom' ? model.vendor : `渠道 ${groups.size + 1}`), url, apiKey, models: [] })
      groups.get(id).models.push(withWorkBuddyDefaults(model, { url, apiKey }))
    })
    channels.value = [...groups.values()]
    selectedKeys.value = new Set(channels.value.flatMap((channel) => channel.models.map((model) => modelKey(channel, model))))
    if (channels.value[0]) {
      selectedChannelId.value = channels.value[0].id
      syncDraft(channels.value[0])
    }
  } catch (e) {
    error.value = e?.message || String(e)
  } finally {
    loading.value = false
  }
}

const syncDraft = (channel) => {
  if (!channel) return
  draft.value = { label: channel.label, url: channel.url, apiKey: channel.apiKey, models: channel.models }
  manualModelId.value = ''
}

const addManualModel = () => {
  const id = manualModelId.value.trim()
  if (!id) return
  const current = draft.value.models || []
  if (!current.some((model) => model.id === id)) {
    draft.value.models = [...current, withWorkBuddyDefaults({ id, name: id, vendor: 'Custom' }, draft.value)]
  }
  manualModelId.value = ''
  if (selectedChannel.value) {
    commitChannel()
    const added = selectedChannel.value.models.find((model) => model.id === id)
    if (added) selectedKeys.value = new Set([...selectedKeys.value, modelKey(selectedChannel.value, added)])
  }
}

const commitChannel = () => {
  error.value = ''
  const url = draft.value.url.trim()
  if (!url) return
  const id = keyForChannel(url, draft.value.apiKey)
  const existing = channels.value.find((channel) => channel.id === id)
  if (selectedChannel.value) {
    const index = channels.value.findIndex((channel) => channel.id === selectedChannel.value.id)
    const apiKey = draft.value.apiKey.trim()
    channels.value[index] = { ...selectedChannel.value, label: draft.value.label.trim() || `渠道 ${index + 1}`, url, apiKey, models: draft.value.models.map((model) => withWorkBuddyDefaults({ ...model, url, apiKey }, { url, apiKey })) }
    selectedChannelId.value = channels.value[index].id
  } else if (existing) {
    if (draft.value.models?.length) {
      const byId = new Map(existing.models.map((model) => [model.id, model]))
      draft.value.models.forEach((model) => byId.set(model.id, model))
      existing.models = [...byId.values()]
    }
    selectedChannelId.value = existing.id
    syncDraft(existing)
  } else {
    const channel = { id, label: draft.value.label.trim() || `渠道 ${channels.value.length + 1}`, url, apiKey: draft.value.apiKey.trim(), models: (draft.value.models || []).map((model) => withWorkBuddyDefaults(model, { url, apiKey: draft.value.apiKey.trim() })) }
    channels.value.push(channel)
    selectedChannelId.value = channel.id
    const next = new Set(selectedKeys.value)
    channel.models.forEach((model) => next.add(modelKey(channel, model)))
    selectedKeys.value = next
  }
  syncDraft(selectedChannel.value)
  saveChannelLabels()
}

const probeModels = async () => {
  probing.value = true
  error.value = ''
  try {
    const found = await invoke('workbuddy_fetch_models', { url: draft.value.url, apiKey: draft.value.apiKey })
    const existing = new Map((draft.value.models || []).map((model) => [model.id, model]))
    draft.value.models = (found || []).map((item) => withWorkBuddyDefaults({
      ...(existing.get(item.id) || {}),
      id: item.id,
      name: existing.get(item.id)?.name || item.name || item.id,
      vendor: existing.get(item.id)?.vendor || item.vendor || 'Custom',
      url: draft.value.url.trim().replace(/\/$/, ''),
      apiKey: draft.value.apiKey.trim()
    }, draft.value))
    commitChannel()
  } catch (e) {
    error.value = e?.message || String(e)
  } finally {
    probing.value = false
  }
}

const toggleModel = (channel, model) => {
  const next = new Set(selectedKeys.value)
  const key = modelKey(channel, model)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  selectedKeys.value = next
}

const selectChannelModels = (checked) => {
  if (!selectedChannel.value) return
  const next = new Set(selectedKeys.value)
  selectedChannel.value.models.forEach((model) => checked ? next.add(modelKey(selectedChannel.value, model)) : next.delete(modelKey(selectedChannel.value, model)))
  selectedKeys.value = next
}

const removeChannel = () => {
  if (!selectedChannel.value) return
  const removed = selectedChannel.value
  channels.value = channels.value.filter((channel) => channel.id !== removed.id)
  const next = new Set(selectedKeys.value)
  removed.models.forEach((model) => next.delete(modelKey(removed, model)))
  selectedKeys.value = next
  startNewChannel()
  saveChannelLabels()
}

const saveModels = async () => {
  saving.value = true
  error.value = ''
  try {
    const byId = new Map()
    selectedModels.value.forEach(({ model, channel }) => byId.set(model.id, withWorkBuddyDefaults({
      ...model,
      url: channel.url,
      apiKey: channel.apiKey
    }, channel)))
    await invoke('workbuddy_save_config', { models: [...byId.values()] })
    window.$notify?.success('WorkBuddy 模型配置已保存')
  } catch (e) {
    error.value = e?.message || String(e)
  } finally {
    saving.value = false
  }
}

onMounted(loadConfig)
</script>
