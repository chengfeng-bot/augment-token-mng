<template>
  <BaseModal
    :visible="true"
    :title="$t('platform.cursor.addAccountDialog.title')"
    :show-close="!isLoading || isWaitingForBrowser"
    :close-on-overlay="(!isLoading || isWaitingForBrowser) && !showConfirmDialog"
    :close-on-esc="(!isLoading || isWaitingForBrowser) && !showConfirmDialog"
    :body-scroll="false"
    modal-class="max-w-[500px]"
    @close="handleClose"
  >
    <!-- Tab 切换 -->
    <div class="mb-6 flex gap-2 rounded-lg bg-muted p-1">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        :class="[
          'flex flex-1 items-center justify-center gap-1.5 rounded-md px-4 py-2.5 text-sm font-medium transition-all',
          activeTab === tab.key
            ? 'bg-surface text-accent shadow-sm'
            : 'text-text-secondary hover:bg-hover hover:text-text'
        ]"
        :disabled="isLoading"
        @click="switchTab(tab.key)"
      >
        {{ tab.label }}
      </button>
    </div>

    <!-- Session Token 方式 -->
    <div v-if="activeTab === 'session'" class="animate-fade-in">
      <div class="mb-5 flex items-start gap-3 rounded-lg border border-accent/20 bg-accent/10 p-4">
        <svg class="mt-0.5 h-5 w-5 shrink-0 text-accent" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/>
        </svg>
        <p class="text-[13px] leading-relaxed text-text-secondary">{{ $t('platform.cursor.addAccountDialog.sessionInfo') }}</p>
      </div>

      <div class="form-group mb-0">
        <label class="label">{{ $t('platform.cursor.addAccountDialog.sessionToken') }}</label>
        <textarea
          v-model="sessionToken"
          :placeholder="$t('platform.cursor.addAccountDialog.sessionTokenPlaceholder')"
          class="input resize-none"
          rows="4"
          :disabled="isLoading || showConfirmDialog"
        ></textarea>
        <p class="mt-1.5 text-xs text-text-muted">
          {{ $t('platform.cursor.addAccountDialog.sessionTokenHint') }}
        </p>
      </div>
    </div>

    <div v-if="isWaitingForBrowser" role="status" class="mt-4 rounded-lg border border-accent/20 bg-accent/10 p-4">
      <div class="flex items-center gap-2 text-sm font-medium text-text">
        <span class="btn-spinner btn-spinner--sm text-accent" aria-hidden="true"></span>
        {{ $t('platform.cursor.addAccountDialog.waitingForBrowser') }}
      </div>
      <p class="mt-2 break-words text-[13px] text-text-secondary">
        {{ $t('platform.cursor.addAccountDialog.browserAccountHint', { email: authAttempt?.email }) }}
      </p>
      <p class="mt-2 text-xs text-text-muted">{{ $t('platform.cursor.addAccountDialog.browserWaitHint') }}</p>
    </div>

    <!-- Access Token 方式 -->
    <div v-if="activeTab === 'accessToken'" class="animate-fade-in">
      <div class="form-group">
        <label class="label">{{ $t('platform.cursor.addAccountDialog.email') }}</label>
        <input
          v-model="accessTokenEmail"
          type="email"
          :placeholder="$t('platform.cursor.addAccountDialog.emailPlaceholder')"
          class="input"
          :disabled="isLoading"
        />
      </div>

      <div class="form-group mb-0">
        <label class="label">Access Token</label>
        <textarea
          v-model="accessToken"
          :placeholder="$t('platform.cursor.addAccountDialog.accessTokenPlaceholder')"
          class="input resize-none"
          rows="4"
          :disabled="isLoading"
        ></textarea>
      </div>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="mt-4 flex items-center gap-2 rounded-lg border border-danger/30 bg-danger/10 p-3 text-[13px] text-danger">
      <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="currentColor">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
      </svg>
      {{ error }}
    </div>

    <!-- 覆盖确认对话框 -->
    <div v-if="showConfirmDialog" class="mt-4 rounded-lg border border-warning/30 bg-warning/10 p-4">
      <div class="flex items-start gap-3">
        <svg class="mt-0.5 h-5 w-5 shrink-0 text-warning" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
        </svg>
        <div class="flex-1">
          <p class="text-sm font-medium text-text">{{ $t('platform.cursor.addAccountDialog.accountExists') }}</p>
          <p class="mt-1 text-xs text-text-secondary">
            {{ $t('platform.cursor.addAccountDialog.accountExistsHint', { email: confirmEmail }) }}
          </p>
        </div>
      </div>
    </div>

    <template #footer>
      <button @click="handleClose" class="btn btn--secondary" :disabled="isLoading && !isWaitingForBrowser">
        {{ $t('common.cancel') }}
      </button>
      <button
        v-if="isWaitingForBrowser"
        @click="reopenBrowser"
        class="btn btn--primary"
        :disabled="isReopening"
      >
        {{ $t('platform.cursor.addAccountDialog.reopenBrowser') }}
      </button>
      <button
        v-else-if="!showConfirmDialog"
        @click="handleAdd"
        class="btn btn--primary"
        :disabled="!canSubmit || isLoading"
      >
        <span class="relative inline-flex items-center justify-center">
          <span :style="{ visibility: isLoading ? 'hidden' : 'visible' }">
            {{ $t('platform.cursor.addAccountDialog.add') }}
          </span>
          <span v-if="isLoading" class="btn-spinner absolute inset-0 m-auto" aria-hidden="true"></span>
        </span>
      </button>
      <button
        v-else
        @click="handleConfirmAdd"
        class="btn btn--primary"
        :disabled="isLoading"
      >
        <span class="relative inline-flex items-center justify-center">
          <span :style="{ visibility: isLoading ? 'hidden' : 'visible' }">
            {{ $t('platform.cursor.addAccountDialog.overwrite') }}
          </span>
          <span v-if="isLoading" class="btn-spinner absolute inset-0 m-auto" aria-hidden="true"></span>
        </span>
      </button>
    </template>
  </BaseModal>
</template>

<script setup>
import { ref, computed, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import BaseModal from '@/components/common/BaseModal.vue'

const { t: $t } = useI18n()

const props = defineProps({
  existingAccounts: {
    type: Array,
    default: () => []
  }
})

const emit = defineEmits(['close', 'added'])

const activeTab = ref('session')
const tabs = computed(() => [
  { key: 'session', label: 'Session Token' },
  { key: 'accessToken', label: 'Access Token' }
])

const sessionToken = ref('')
const accessToken = ref('')
const accessTokenEmail = ref('')
const isLoading = ref(false)
const error = ref('')
const showConfirmDialog = ref(false)
const existingAccountId = ref(null)
const confirmEmail = ref('')
const authAttempt = ref(null)
const isWaitingForBrowser = ref(false)
const isReopening = ref(false)
let activeOperation = 0
let disposed = false
let authDeadline = 0
let pollTimer
let resolvePollDelay

const isCurrent = operation => !disposed && operation === activeOperation

const discardAttempt = () => {
  clearTimeout(pollTimer)
  resolvePollDelay?.()
  resolvePollDelay = null
  const loginId = authAttempt.value?.login_id
  authAttempt.value = null
  isWaitingForBrowser.value = false
  isReopening.value = false
  if (loginId) invoke('cursor_cancel_browser_login', { loginId }).catch(() => {})
}

const formatAuthError = err => {
  const message = err?.message || String(err)
  const keys = {
    CURSOR_AUTH_SESSION_CHECK_FAILED: 'sessionCheckFailed',
    CURSOR_AUTH_IDENTITY_UNAVAILABLE: 'identityUnavailable',
    CURSOR_AUTH_EXPIRED: 'authExpired',
    CURSOR_AUTH_ACCOUNT_MISMATCH: 'authAccountMismatch',
    CURSOR_AUTH_ACCOUNT_EXISTS: 'authAccountExists',
    CURSOR_AUTH_BROWSER_OPEN_FAILED: 'browserOpenFailed',
    CURSOR_AUTH_DENIED: 'authDenied',
    CURSOR_AUTH_INVALID_RESPONSE: 'authInvalidResponse',
    CURSOR_AUTH_NOT_READY: 'authNotReady',
    CURSOR_AUTH_BUSY: 'authBusy',
    CURSOR_AUTH_FAILED: 'authFailed'
  }
  return keys[message]
    ? $t(`platform.cursor.addAccountDialog.${keys[message]}`, { email: authAttempt.value?.email || confirmEmail.value })
    : message
}

const canSubmit = computed(() => {
  if (activeTab.value === 'session') {
    return sessionToken.value.trim().length > 0
  }
  return accessTokenEmail.value.trim().length > 0 && accessToken.value.trim().length > 0
})

const switchTab = (tab) => {
  if (isLoading.value) return
  activeOperation += 1
  discardAttempt()
  activeTab.value = tab
  error.value = ''
  showConfirmDialog.value = false
  existingAccountId.value = null
  confirmEmail.value = ''
  accessTokenEmail.value = ''
}

const handleClose = () => {
  if (isLoading.value && !isWaitingForBrowser.value) return
  activeOperation += 1
  discardAttempt()
  isLoading.value = false
  emit('close')
}

const checkEmailExists = (email) => {
  if (!props.existingAccounts || props.existingAccounts.length === 0) {
    return null
  }
  const normalizedEmail = email.toLowerCase().trim()
  return props.existingAccounts.find(acc => acc.email.toLowerCase().trim() === normalizedEmail)
}

const handleAdd = async () => {
  if (!canSubmit.value || isLoading.value) return

  const operation = ++activeOperation
  error.value = ''
  isLoading.value = true
  existingAccountId.value = null

  try {
    if (activeTab.value === 'session') {
      await handleAddWithSession(operation)
    } else {
      await handleAddWithAccessToken(operation)
    }
  } catch (err) {
    if (isCurrent(operation)) {
      error.value = formatAuthError(err)
      discardAttempt()
      showConfirmDialog.value = false
    }
  } finally {
    if (isCurrent(operation)) isLoading.value = false
  }
}

const handleAddWithSession = async operation => {
  const attempt = await invoke('cursor_begin_browser_login', {
    sessionToken: sessionToken.value.trim()
  })
  if (!isCurrent(operation)) {
    invoke('cursor_cancel_browser_login', { loginId: attempt.login_id }).catch(() => {})
    return
  }
  authAttempt.value = attempt
  authDeadline = Date.now() + attempt.expires_in_seconds * 1000
  confirmEmail.value = attempt.email

  const existing = checkEmailExists(attempt.email)
  if (existing) {
    existingAccountId.value = existing.id
    showConfirmDialog.value = true
    return
  }

  await authorizeAndSave(operation)
}

const handleAddWithAccessToken = async operation => {
  const account = await invoke('cursor_add_account_with_access_token', {
    email: accessTokenEmail.value.trim(),
    accessToken: accessToken.value.trim()
  })

  if (isCurrent(operation)) emit('added', account)
}

const handleConfirmAdd = async () => {
  if (isLoading.value || !authAttempt.value) return
  const operation = activeOperation
  error.value = ''
  isLoading.value = true

  try {
    await authorizeAndSave(operation)
  } catch (err) {
    if (isCurrent(operation)) {
      error.value = formatAuthError(err)
      discardAttempt()
      showConfirmDialog.value = false
    }
  } finally {
    if (isCurrent(operation)) isLoading.value = false
  }
}

const reopenBrowser = async () => {
  if (!authAttempt.value || isReopening.value || !isWaitingForBrowser.value) return
  const operation = activeOperation
  isReopening.value = true
  try {
    await invoke('cursor_open_browser_login', { loginId: authAttempt.value.login_id })
    if (isCurrent(operation) && isWaitingForBrowser.value) error.value = ''
  } catch (err) {
    if (isCurrent(operation) && isWaitingForBrowser.value) error.value = formatAuthError(err)
  } finally {
    if (isCurrent(operation)) isReopening.value = false
  }
}

const authorizeAndSave = async operation => {
  const loginId = authAttempt.value.login_id
  showConfirmDialog.value = false
  isWaitingForBrowser.value = true
  await reopenBrowser()
  while (isCurrent(operation)) {
    if (Date.now() >= authDeadline) throw new Error('CURSOR_AUTH_EXPIRED')
    const ready = await invoke('cursor_poll_browser_login', { loginId })
    if (!isCurrent(operation)) return
    if (ready) {
      // Saving is a separate step, so a cancelled poll can never write an account.
      isWaitingForBrowser.value = false
      const account = existingAccountId.value
        ? await invoke('cursor_refresh_account_tokens', { accountId: existingAccountId.value, loginId })
        : await invoke('cursor_add_account_with_session', { loginId })
      if (isCurrent(operation)) {
        authAttempt.value = null
        emit('added', account)
      }
      return
    }
    await new Promise(resolve => {
      resolvePollDelay = resolve
      pollTimer = setTimeout(() => { resolvePollDelay = null; resolve() }, 2000)
    })
  }
}

onUnmounted(() => {
  disposed = true
  activeOperation += 1
  discardAttempt()
})
</script>

<style scoped>
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
