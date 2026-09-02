<template>
  <BaseModal
    :visible="true"
    :title="$t('platform.antigravity.addAccountDialog.title')"
    :show-close="true"
    :close-on-overlay="!isLoading && !isImporting"
    :close-on-esc="!isLoading && !isImporting"
    :body-scroll="false"
    modal-class="max-w-[500px]"
    @close="handleClose"
  >
    <!-- 添加方式选择 -->
    <div class="mb-6 flex gap-2 rounded-lg bg-muted p-1">
      <button
        :class="[
          'flex flex-1 items-center justify-center gap-1.5 rounded-md px-4 py-2.5 text-sm font-medium transition-all disabled:cursor-not-allowed disabled:opacity-50',
          addMethod === 'oauth'
            ? 'bg-surface text-accent shadow-sm'
            : 'text-text-secondary hover:bg-hover hover:text-text'
        ]"
        @click="switchToOAuth"
        :disabled="isImporting"
      >
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
        </svg>
        {{ $t('platform.antigravity.addAccountDialog.oauthMethod') }}
      </button>
      <button
        :class="[
          'flex flex-1 items-center justify-center gap-1.5 rounded-md px-4 py-2.5 text-sm font-medium transition-all disabled:cursor-not-allowed disabled:opacity-50',
          addMethod === 'manual'
            ? 'bg-surface text-accent shadow-sm'
            : 'text-text-secondary hover:bg-hover hover:text-text'
        ]"
        @click="addMethod = 'manual'"
        :disabled="isImporting"
      >
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/>
        </svg>
        {{ $t('platform.antigravity.addAccountDialog.manualMethod') }}
      </button>
      <button
        :class="[
          'flex flex-1 items-center justify-center gap-1.5 rounded-md px-4 py-2.5 text-sm font-medium transition-all disabled:cursor-not-allowed disabled:opacity-50',
          addMethod === 'import'
            ? 'bg-surface text-accent shadow-sm'
            : 'text-text-secondary hover:bg-hover hover:text-text'
        ]"
        @click="addMethod = 'import'"
        :disabled="isImporting"
      >
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14 13v4h-4v-4H7l5-5 5 5h-3z"/>
        </svg>
        {{ $t('platform.antigravity.addAccountDialog.importMethod') }}
      </button>
    </div>

    <!-- OAuth 授权方式 -->
    <div v-if="addMethod === 'oauth'" class="animate-fade-in">
      <!-- OAuth Info -->
      <div class="mb-5 flex items-start gap-3 rounded-lg border border-accent/20 bg-accent/10 p-4">
        <svg class="mt-0.5 h-5 w-5 shrink-0 text-accent" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/>
        </svg>
        <p class="text-[13px] leading-relaxed text-text-secondary">{{ $t('platform.antigravity.addAccountDialog.oauthInfo') }}</p>
      </div>

      <!-- Google OAuth Button -->
      <button
        @click="handleOAuthLogin"
        class="flex w-full items-center justify-center gap-2.5 rounded-lg border border-border bg-white px-5 py-3.5 text-[15px] font-medium text-neutral-800 transition-all hover:border-border-strong hover:bg-neutral-50 hover:shadow-sm disabled:cursor-not-allowed disabled:opacity-60"
        :disabled="isLoading"
      >
        <span class="relative inline-flex h-5 w-5 items-center justify-center">
          <svg :style="{ visibility: isLoading ? 'hidden' : 'visible' }" class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12.545,10.239v3.821h5.445c-0.712,2.315-2.647,3.972-5.445,3.972c-3.332,0-6.033-2.701-6.033-6.032s2.701-6.032,6.033-6.032c1.498,0,2.866,0.549,3.921,1.453l2.814-2.814C17.503,2.988,15.139,2,12.545,2C7.021,2,2.543,6.477,2.543,12s4.478,10,10.002,10c8.396,0,10.249-7.85,9.426-11.748L12.545,10.239z"/>
          </svg>
          <span v-if="isLoading" class="btn-spinner absolute inset-0 m-auto" aria-hidden="true"></span>
        </span>
        {{ isLoading ? $t('platform.antigravity.addAccountDialog.adding') : $t('platform.antigravity.addAccountDialog.googleLogin') }}
      </button>

      <!-- Manual OAuth Section -->
      <div class="mt-5 rounded-lg bg-muted p-4">
        <div class="mb-3 text-[13px] font-semibold text-text">{{ $t('platform.antigravity.addAccountDialog.oauthManualTitle') }}</div>

        <div class="mb-3 flex gap-2.5">
          <button class="btn btn--primary" @click="generateAuthUrl" :disabled="isLoading || isManualLoading">
            {{ $t('platform.antigravity.addAccountDialog.generateAuthLink') }}
          </button>
        </div>

        <div v-if="oauthAuthUrl" class="mb-3 flex items-center gap-2">
          <input type="text" :value="oauthAuthUrl" readonly class="input flex-1" />
          <button
            class="btn btn--secondary btn--icon shrink-0"
            @click="copyAuthUrl"
            :disabled="isLoading || isManualLoading"
            v-tooltip="$t('platform.antigravity.addAccountDialog.copyAuthLink')"
          >
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/>
            </svg>
          </button>
        </div>

        <div class="form-group mb-3">
          <label class="label">{{ $t('platform.antigravity.addAccountDialog.callbackLabel') }}</label>
          <div class="relative flex items-center">
            <input
              v-model="oauthCallbackInput"
              type="text"
              :placeholder="$t('platform.antigravity.addAccountDialog.callbackPlaceholder')"
              class="input w-full pr-9"
              :disabled="isLoading || isManualLoading"
            />
            <button
              v-if="oauthCallbackInput"
              class="absolute right-1.5 flex h-7 w-7 items-center justify-center rounded text-text-muted transition-colors hover:bg-hover hover:text-text"
              type="button"
              @click="oauthCallbackInput = ''"
            >
              <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor">
                <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
              </svg>
            </button>
          </div>
          <p class="mt-1.5 text-xs text-text-muted">
            {{ $t('platform.antigravity.addAccountDialog.callbackHint') }}
          </p>
        </div>

        <button class="btn btn--primary" @click="handleOAuthExchange" :disabled="!canExchange || isLoading || isManualLoading">
          {{ $t('platform.antigravity.addAccountDialog.exchangeCode') }}
        </button>
      </div>
    </div>

    <!-- 手动添加方式 -->
    <div v-else-if="addMethod === 'manual'" class="animate-fade-in">
      <div class="form-group">
        <label class="label">{{ $t('platform.antigravity.addAccountDialog.email') }}</label>
        <input
          v-model="email"
          type="email"
          :placeholder="$t('platform.antigravity.addAccountDialog.emailPlaceholder')"
          class="input"
          :disabled="isLoading"
        />
      </div>

      <div class="form-group mb-0">
        <label class="label">{{ $t('platform.antigravity.addAccountDialog.refreshToken') }}</label>
        <textarea
          v-model="refreshToken"
          :placeholder="$t('platform.antigravity.addAccountDialog.refreshTokenPlaceholder')"
          class="input resize-none"
          rows="4"
          :disabled="isLoading"
        ></textarea>
        <p class="mt-1.5 text-xs text-text-muted">
          {{ $t('platform.antigravity.addAccountDialog.refreshTokenHint') }}
        </p>
      </div>
    </div>

    <!-- 导入方式 -->
    <div v-else-if="addMethod === 'import'" class="animate-fade-in">
      <div class="mb-5 flex items-start gap-3 rounded-lg border border-accent/20 bg-accent/10 p-4">
        <svg class="mt-0.5 h-5 w-5 shrink-0 text-accent" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/>
        </svg>
        <p class="text-[13px] leading-relaxed text-text-secondary">{{ $t('platform.antigravity.importDialog.info') }}</p>
        <button
          @click="showFormatModal = true"
          class="shrink-0 rounded p-1 text-text-muted hover:bg-accent/20 hover:text-accent transition-colors"
          v-tooltip="$t('platform.antigravity.importDialog.formatExample')"
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 17h-2v-2h2v2zm2.07-7.75l-.9.92C13.45 12.9 13 13.5 13 15h-2v-.5c0-1.1.45-2.1 1.17-2.83l1.24-1.26c.37-.36.59-.86.59-1.41 0-1.1-.9-2-2-2s-2 .9-2 2H8c0-2.21 1.79-4 4-4s4 1.79 4 4c0 .88-.36 1.68-.93 2.25z"/>
          </svg>
        </button>
      </div>

      <input
        ref="fileInputRef"
        type="file"
        accept=".json"
        class="hidden"
        @change="handleFileChange"
      />

      <div v-if="!previewItems.length" class="form-group mb-0">
        <label class="label">{{ $t('platform.antigravity.importDialog.selectFile') }}</label>
        <div
          class="flex flex-col items-center justify-center gap-3 rounded-lg border-2 border-dashed border-border p-8 cursor-pointer hover:border-accent hover:bg-accent/5 transition-colors"
          @click="selectFile"
        >
          <svg class="h-10 w-10 text-text-muted" viewBox="0 0 24 24" fill="currentColor">
            <path d="M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14 13v4h-4v-4H7l5-5 5 5h-3z"/>
          </svg>
          <span class="text-sm text-text-secondary">{{ $t('platform.antigravity.importDialog.clickToSelect') }}</span>
          <span class="text-xs text-text-muted">{{ $t('platform.antigravity.importDialog.supportFormat') }}</span>
        </div>
      </div>

      <div v-else class="space-y-4">
        <div class="flex items-center justify-between rounded-lg bg-surface-alt p-3">
          <div class="flex items-center gap-2">
            <svg class="h-5 w-5 text-accent" viewBox="0 0 24 24" fill="currentColor">
              <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z"/>
            </svg>
            <span class="text-sm text-text">{{ fileName }}</span>
          </div>
          <button
            @click="clearFile"
            class="text-xs text-text-muted hover:text-danger transition-colors"
            :disabled="isImporting"
          >
            {{ $t('platform.antigravity.importDialog.reselect') }}
          </button>
        </div>

        <div class="rounded-lg border border-border">
          <div class="flex items-center justify-between border-b border-border px-4 py-2 bg-surface-alt">
            <span class="text-sm font-medium text-text">
              {{ $t('platform.antigravity.importDialog.previewTitle', { count: previewItems.length }) }}
            </span>
          </div>
          <div class="max-h-[300px] overflow-y-auto">
            <div
              v-for="(item, index) in previewItems"
              :key="index"
              class="flex items-center gap-2 px-4 py-2.5 border-b border-border last:border-b-0"
            >
              <span class="text-xs text-text-muted shrink-0">#{{ index + 1 }}</span>
              <span v-if="item.email" class="text-sm text-text truncate">{{ item.email }}</span>
              <span v-else class="text-sm text-text truncate font-mono">{{ maskToken(item.refreshToken) }}</span>
            </div>
          </div>
        </div>

        <div v-if="importResult" class="rounded-lg border border-border p-4">
          <div class="flex items-center justify-between mb-3">
            <span class="text-sm font-medium text-text">{{ $t('platform.antigravity.importDialog.resultTitle') }}</span>
            <span class="text-xs text-text-muted">
              {{ $t('platform.antigravity.importDialog.resultSummary', {
                success: importResult.success_count,
                failed: importResult.failed_count,
                total: importResult.total
              }) }}
            </span>
          </div>
          <div class="max-h-[200px] overflow-y-auto space-y-1">
            <div
              v-for="(result, index) in importResult.results"
              :key="index"
              class="flex items-center gap-2 text-xs py-1"
            >
              <svg
                v-if="result.success"
                class="h-4 w-4 shrink-0 text-success"
                viewBox="0 0 24 24"
                fill="currentColor"
              >
                <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
              </svg>
              <svg
                v-else
                class="h-4 w-4 shrink-0 text-danger"
                viewBox="0 0 24 24"
                fill="currentColor"
              >
                <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
              </svg>
              <span class="text-text truncate">{{ result.email || `#${index + 1}` }}</span>
              <span v-if="result.error" class="text-danger truncate">{{ result.error }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="mt-4 flex items-center gap-2 rounded-lg border border-danger/30 bg-danger/10 p-3 text-[13px] text-danger">
      <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="currentColor">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
      </svg>
      {{ error }}
    </div>

    <template #footer>
      <button @click="handleClose" class="btn btn--secondary" :disabled="isLoading || isImporting">
        {{ (addMethod === 'import' && importResult) ? $t('common.close') : $t('common.cancel') }}
      </button>
      <button
        v-if="addMethod === 'manual'"
        @click="handleAdd"
        class="btn btn--primary"
        :disabled="!canSubmit || isLoading"
      >
        <span class="relative inline-flex items-center justify-center">
          <span :style="{ visibility: isLoading ? 'hidden' : 'visible' }">
            {{ $t('platform.antigravity.addAccountDialog.add') }}
          </span>
          <span v-if="isLoading" class="btn-spinner absolute inset-0 m-auto" aria-hidden="true"></span>
        </span>
      </button>
      <button
        v-if="addMethod === 'import' && previewItems.length && !importResult"
        @click="handleImport"
        class="btn btn--primary"
        :disabled="isImporting"
      >
        <span class="relative inline-flex items-center justify-center">
          <span :style="{ visibility: isImporting ? 'hidden' : 'visible' }">
            {{ $t('platform.antigravity.importDialog.import', { count: previewItems.length }) }}
          </span>
          <span v-if="isImporting" class="btn-spinner absolute inset-0 m-auto" aria-hidden="true"></span>
        </span>
      </button>
    </template>
  </BaseModal>

  <BaseModal
    :visible="showFormatModal"
    :title="$t('platform.antigravity.importDialog.formatExample')"
    :show-close="true"
    close-on-overlay
    close-on-esc
    modal-class="max-w-[500px]"
    @close="showFormatModal = false"
  >
    <div class="space-y-4">
      <pre class="rounded-lg bg-surface-alt p-4 text-xs text-text overflow-x-auto">{{ formatExampleJson }}</pre>
    </div>
    <template #footer>
      <button @click="showFormatModal = false" class="btn btn--primary">{{ $t('common.close') }}</button>
    </template>
  </BaseModal>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import BaseModal from '@/components/common/BaseModal.vue'

const { t: $t } = useI18n()
const emit = defineEmits(['close', 'add', 'added', 'imported'])

const handleClose = () => {
  if (isLoading.value || isImporting.value) return
  emit('close')
}

const addMethod = ref('oauth') // 'oauth', 'manual', or 'import'
const email = ref('')
const refreshToken = ref('')
const isLoading = ref(false)
const isManualLoading = ref(false)
const error = ref('')
const oauthAuthUrl = ref('')
const oauthRedirectUri = ref('')
const oauthCallbackInput = ref('')

const showFormatModal = ref(false)
const fileName = ref('')
const previewItems = ref([])
const importResult = ref(null)
const isImporting = ref(false)
const fileInputRef = ref(null)
const formatExampleJson = `// 格式 1：Refresh Token 数组
[
  "refresh_token_1",
  "refresh_token_2"
]

// 格式 2：ATM 导出格式
[
  {
    "email": "user@example.com",
    "token": {
      "refresh_token": "...",
      "access_token": "..."
    }
  }
]

// 格式 3：email + refresh_token 对象数组
[
  {
    "email": "user@example.com",
    "refresh_token": "..."
  }
]`

const canSubmit = computed(() => {
  if (addMethod.value === 'oauth') return true
  return email.value.trim() && refreshToken.value.trim()
})

const canExchange = computed(() => {
  const raw = oauthCallbackInput.value.trim()
  if (!raw) return false
  if (/^https?:\/\//i.test(raw)) return true
  return !!oauthRedirectUri.value
})

const resetOAuthState = () => {
  oauthAuthUrl.value = ''
  oauthRedirectUri.value = ''
  oauthCallbackInput.value = ''
}

const switchToOAuth = () => {
  addMethod.value = 'oauth'
  resetOAuthState()
}

let unlistenOAuthUrl = null

onMounted(async () => {
  unlistenOAuthUrl = await listen('oauth-url-generated', event => {
    const url = typeof event.payload === 'string' ? event.payload : ''
    if (!url) return
    oauthAuthUrl.value = url
    try {
      const parsed = new URL(url)
      const redirect = parsed.searchParams.get('redirect_uri')
      if (redirect) {
        oauthRedirectUri.value = redirect
      }
    } catch (err) {
      console.error('Parse oauth url error:', err)
    }
  })
})

onUnmounted(() => {
  if (unlistenOAuthUrl) {
    unlistenOAuthUrl()
    unlistenOAuthUrl = null
  }
})

const handleOAuthLogin = async () => {
  error.value = ''
  isLoading.value = true
  resetOAuthState()

  try {
    // 使用 OAuth 服务器模式，自动完成整个流程
    const account = await invoke('antigravity_start_oauth_login')

    emit('added', account)

  } catch (err) {
    console.error('OAuth login error:', err)
    error.value = formatOAuthError(err)
  } finally {
    isLoading.value = false
  }
}

const generateAuthUrl = async () => {
  error.value = ''
  isManualLoading.value = true

  try {
    const port = Math.floor(Math.random() * 16383) + 49152
    const redirectUri = `http://localhost:${port}/oauth-callback`
    const authUrl = await invoke('antigravity_get_auth_url', { redirectUri })
    oauthAuthUrl.value = authUrl
    oauthRedirectUri.value = redirectUri
  } catch (err) {
    console.error('Generate auth url error:', err)
    error.value = formatOAuthError(err)
  } finally {
    isManualLoading.value = false
  }
}

const copyAuthUrl = async () => {
  if (!oauthAuthUrl.value) return

  try {
    await navigator.clipboard.writeText(oauthAuthUrl.value)
    window.$notify?.success($t('platform.antigravity.addAccountDialog.authLinkCopied'))
  } catch (err) {
    console.error('Copy auth url error:', err)
    error.value = err?.message || err || '复制授权链接失败'
  }
}

const formatOAuthError = (err) => {
  const message = err?.message || err || ''
  if (message.includes('ANTIGRAVITY_EMAIL_EXISTS')) {
    return $t('platform.antigravity.addAccountDialog.emailExists')
  }
  if (message.includes('ANTIGRAVITY_OAUTH_CLIENT_ID_NOT_CONFIGURED')) {
    return '未配置 ATM_ANTIGRAVITY_CLIENT_ID'
  }
  if (message.includes('ANTIGRAVITY_OAUTH_CLIENT_SECRET_NOT_CONFIGURED')) {
    return '未配置 ATM_ANTIGRAVITY_CLIENT_SECRET'
  }
  return message || $t('platform.antigravity.addAccountDialog.oauthExchangeFailed')
}

const handleOAuthExchange = async () => {
  const rawInput = oauthCallbackInput.value.trim()
  if (!rawInput) return

  error.value = ''
  isManualLoading.value = true

  try {
    let code = rawInput
    let redirectUri = oauthRedirectUri.value

    if (/^https?:\/\//i.test(rawInput)) {
      const url = new URL(rawInput)
      code = url.searchParams.get('code') || ''
      redirectUri = `${url.origin}${url.pathname}`
    }

    if (!code) {
      throw new Error($t('platform.antigravity.addAccountDialog.invalidCallback'))
    }

    if (!redirectUri) {
      throw new Error($t('platform.antigravity.addAccountDialog.missingRedirectUri'))
    }

    const account = await invoke('antigravity_exchange_code', { code, redirectUri })
    emit('added', account)
  } catch (err) {
    console.error('Exchange code error:', err)
    error.value = formatOAuthError(err)
  } finally {
    isManualLoading.value = false
  }
}

const handleAdd = async () => {
  if (!canSubmit.value) return

  error.value = ''
  isLoading.value = true

  try {
    await emit('add', email.value.trim(), refreshToken.value.trim())
  } catch (err) {
    console.error('Add account error:', err)
    error.value = err?.message || err || '添加账号失败'
  } finally {
    isLoading.value = false
  }
}

const extractAccount = (item) => {
  if (typeof item === 'string' && item.trim()) {
    return { email: '', refreshToken: item.trim() }
  }
  if (!item || typeof item !== 'object') return null

  const token = typeof item.refresh_token === 'string'
    ? item.refresh_token
    : typeof item.refreshToken === 'string'
      ? item.refreshToken
      : item.token?.refresh_token
  if (typeof token !== 'string' || !token.trim()) return null

  const accountEmail = item.email || item.token?.email || ''
  return {
    email: typeof accountEmail === 'string' ? accountEmail : '',
    refreshToken: token.trim()
  }
}

const parseImportData = (data) => {
  const raw = data && typeof data === 'object' && !Array.isArray(data) && Array.isArray(data.accounts)
    ? data.accounts
    : data

  const items = []
  if (Array.isArray(raw)) {
    for (const entry of raw) {
      const extracted = extractAccount(entry)
      if (extracted) items.push(extracted)
    }
  } else {
    const extracted = extractAccount(raw)
    if (extracted) items.push(extracted)
  }
  return items
}

const maskToken = (token) => {
  if (!token || token.length < 12) return '***'
  return token.slice(0, 6) + '...' + token.slice(-4)
}

const selectFile = () => {
  fileInputRef.value?.click()
}

const handleFileChange = async (event) => {
  const file = event.target.files?.[0]
  if (!file) return

  try {
    const content = await file.text()
    const data = JSON.parse(content)
    const items = parseImportData(data)

    if (items.length === 0) {
      error.value = $t('platform.antigravity.importDialog.emptyFile')
      return
    }

    fileName.value = file.name
    previewItems.value = items
    importResult.value = null
    error.value = ''
  } catch (err) {
    console.error('Failed to read file:', err)
    error.value = err?.message || $t('platform.antigravity.importDialog.readError')
  } finally {
    event.target.value = ''
  }
}

const clearFile = () => {
  fileName.value = ''
  previewItems.value = []
  importResult.value = null
  error.value = ''
}

const handleImport = async () => {
  if (!previewItems.value.length || isImporting.value) return

  error.value = ''
  isImporting.value = true
  const results = []
  const accountIds = []
  let success_count = 0
  let failed_count = 0

  try {
    for (const item of previewItems.value) {
      try {
        const account = await invoke('antigravity_add_account', {
          email: item.email || '',
          refreshToken: item.refreshToken
        })
        results.push({ success: true, email: account?.email || item.email || '' })
        if (account?.id) accountIds.push(account.id)
        success_count++
      } catch (err) {
        const msg = err?.message || err || String(err)
        results.push({ success: false, email: item.email || '', error: msg })
        failed_count++
      }
    }

    importResult.value = {
      success_count,
      failed_count,
      total: previewItems.value.length,
      results
    }

    if (success_count > 0) {
      emit('imported', { ...importResult.value, accountIds })
    }
  } catch (err) {
    console.error('Import error:', err)
    error.value = err?.message || err || $t('platform.antigravity.importDialog.importFailed')
  } finally {
    isImporting.value = false
  }
}
</script>

<style scoped>
/* Fade-in animation for tab content */
.animate-fade-in {
  animation: fadeIn 0.3s ease-out;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(-10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
</style>
