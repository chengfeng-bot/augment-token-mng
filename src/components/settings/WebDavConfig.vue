<template>
  <BaseModal
    :visible="true"
    :title="$t('webdavConfig.title')"
    modal-class="w-[min(760px,calc(100vw-32px))]"
    @close="$emit('close')"
  >
    <div class="flex flex-col gap-5">
      <div class="grid gap-4 md:grid-cols-3">
        <div class="form-group !mb-0">
          <label class="label">{{ $t('webdavConfig.vendor') }}</label>
          <div ref="vendorDropdownRef" class="dropdown w-full">
            <button
              type="button"
              class="btn btn--secondary w-full justify-between"
              :disabled="isBusy"
              :aria-expanded="showVendorMenu ? 'true' : 'false'"
              aria-haspopup="listbox"
              @click="toggleVendorMenu"
            >
              <span>{{ vendorLabel }}</span>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="6 9 12 15 18 9"/>
              </svg>
            </button>
            <div v-if="showVendorMenu" class="dropdown-menu left-0 z-50 w-full" role="listbox">
              <button
                v-for="option in vendorOptions"
                :key="option.value"
                type="button"
                class="dropdown-item"
                role="option"
                @click="selectVendor(option)"
              >
                {{ option.label }}
              </button>
            </div>
          </div>
          <small class="mt-1 block text-[12px] text-text-muted">{{ vendorHint }}</small>
        </div>

        <div class="form-group !mb-0">
          <label for="remoteDir" class="label">{{ $t('webdavConfig.remoteDir') }}</label>
          <input
            id="remoteDir"
            v-model="config.remoteDir"
            type="text"
            class="input"
            :placeholder="$t('webdavConfig.placeholders.remoteDir')"
            :disabled="isBusy"
          >
        </div>

        <div class="form-group !mb-0">
          <label for="retentionCount" class="label">{{ $t('webdavConfig.retentionCount') }}</label>
          <input
            id="retentionCount"
            v-model.number="config.retentionCount"
            type="number"
            min="1"
            max="30"
            class="input"
            :disabled="isBusy"
          >
        </div>
      </div>

      <div class="form-group !mb-0">
        <label for="url" class="label">{{ $t('webdavConfig.url') }}</label>
        <input
          id="url"
          v-model="config.url"
          type="url"
          class="input"
          :placeholder="$t('webdavConfig.placeholders.url')"
          :disabled="isBusy"
        >
      </div>

      <div class="grid gap-4 md:grid-cols-2">
        <div class="form-group !mb-0">
          <label for="username" class="label">{{ $t('webdavConfig.username') }}</label>
          <input
            id="username"
            v-model="config.username"
            type="text"
            class="input"
            :placeholder="$t('webdavConfig.placeholders.username')"
            :disabled="isBusy"
          >
        </div>

        <div class="form-group !mb-0">
          <label for="password" class="label flex items-center gap-1.5">
            <span>{{ $t('webdavConfig.password') }}</span>
            <span
              class="group relative inline-flex h-4 w-4 cursor-help items-center justify-center rounded-full border border-border text-[11px] font-semibold text-text-muted"
              tabindex="0"
              :aria-label="$t('webdavConfig.passwordHelpAria')"
            >
              ?
              <span class="pointer-events-none absolute left-1/2 top-6 z-50 hidden w-72 -translate-x-1/2 rounded-lg border border-border bg-surface p-3 text-left text-xs font-normal leading-relaxed text-text shadow-lg group-hover:block group-focus:block">
                {{ $t('webdavConfig.passwordHelp') }}
              </span>
            </span>
            <span v-if="hasExistingPassword" class="text-text-muted">({{ $t('webdavConfig.passwordSaved') }})</span>
          </label>
          <input
            id="password"
            v-model="config.password"
            type="password"
            class="input"
            :placeholder="passwordPlaceholder"
            :disabled="isBusy"
          >
        </div>
      </div>

      <label class="flex cursor-pointer items-center gap-2 text-sm font-medium text-text">
        <input
          v-model="config.enabled"
          type="checkbox"
          class="h-[18px] w-[18px] cursor-pointer accent-accent"
          :disabled="isBusy"
        >
        <span>{{ $t('webdavConfig.enable') }}</span>
      </label>

      <div class="rounded-lg border border-border bg-surface p-4">
        <div class="mb-3 flex items-center justify-between gap-3">
          <div>
            <h4 class="text-sm font-semibold text-text">{{ $t('webdavConfig.backupTitle') }}</h4>
            <p class="mt-1 text-xs text-text-muted">{{ $t('webdavConfig.backupHint', { count: normalizedRetentionCount }) }}</p>
          </div>
          <button
            class="btn btn--secondary btn--sm"
            :disabled="!config.enabled || isBusy"
            @click="refreshBackups"
          >
            <span v-if="isRefreshingBackups" class="btn-spinner" aria-hidden="true"></span>
            {{ $t('webdavConfig.refreshBackups') }}
          </button>
        </div>

        <div class="grid gap-3 md:grid-cols-[1fr_auto]">
          <input
            v-model="backupPassphrase"
            type="password"
            class="input"
            :placeholder="$t('webdavConfig.placeholders.backupPassphrase')"
            :disabled="!config.enabled || isBusy"
          >
          <button
            class="btn btn--primary"
            :disabled="!canBackup"
            @click="backupNow"
          >
            <span v-if="settingsStore.isBackingUpWebdav" class="btn-spinner" aria-hidden="true"></span>
            {{ $t('webdavConfig.backupNow') }}
          </button>
        </div>

        <div class="mt-3 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-xs leading-relaxed text-text">
          {{ $t('webdavConfig.passphraseWarning') }}
        </div>

        <div class="mt-4">
          <div v-if="backups.length === 0" class="rounded-md border border-dashed border-border px-3 py-5 text-center text-sm text-text-muted">
            {{ $t('webdavConfig.noBackups') }}
          </div>
          <div v-else class="flex flex-col gap-2">
            <div
              v-for="backup in backups"
              :key="backup.name"
              class="flex flex-col gap-3 rounded-md border border-border bg-muted/20 p-3 md:flex-row md:items-center md:justify-between"
            >
              <div class="min-w-0">
                <div class="truncate font-mono text-xs text-text">{{ backup.name }}</div>
                <div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-text-muted">
                  <span>{{ formatDate(backup.modifiedAt) }}</span>
                  <span>{{ formatBytes(backup.size) }}</span>
                </div>
              </div>
              <div class="flex shrink-0 items-center gap-2">
                <button
                  class="btn btn--primary btn--sm"
                  :disabled="isBusy"
                  @click="restoreBackup(backup)"
                >
                  <span v-if="restoringName === backup.name" class="btn-spinner" aria-hidden="true"></span>
                  {{ $t('webdavConfig.restore') }}
                </button>
                <button
                  class="btn btn--danger btn--sm"
                  :disabled="isBusy"
                  @click="deleteBackup(backup)"
                >
                  <span v-if="deletingName === backup.name" class="btn-spinner" aria-hidden="true"></span>
                  {{ $t('webdavConfig.deleteBackup') }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="mr-auto flex items-center gap-2.5">
        <button
          class="btn btn--secondary"
          :disabled="!canTest || isBusy"
          @click="testConnection"
        >
          <span v-if="settingsStore.isTestingWebdav" class="btn-spinner" aria-hidden="true"></span>
          {{ $t('webdavConfig.testConnection') }}
        </button>
        <span v-if="lastTestOk" class="badge badge--success">
          <span class="status-dot"></span>
          {{ $t('webdavConfig.tested') }}
        </span>
      </div>

      <button
        class="btn btn--primary"
        :disabled="!canSave || isBusy"
        @click="saveConfig"
      >
        <span v-if="isSaving" class="btn-spinner" aria-hidden="true"></span>
        {{ $t('webdavConfig.save') }}
      </button>

      <button
        v-if="hasExistingConfig"
        class="btn btn--danger"
        :disabled="isBusy"
        @click="deleteConfig"
      >
        {{ $t('webdavConfig.delete') }}
      </button>
    </template>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '../../stores/settings'
import BaseModal from '../common/BaseModal.vue'

const emit = defineEmits(['close', 'saved', 'deleted'])
const { t } = useI18n()
const settingsStore = useSettingsStore()

const config = ref({
  enabled: false,
  vendor: 'jianguoyun',
  url: 'https://dav.jianguoyun.com/dav/',
  username: '',
  password: '',
  remoteDir: 'ATM',
  retentionCount: 1
})

const hasExistingConfig = ref(false)
const hasExistingPassword = ref(false)
const showVendorMenu = ref(false)
const vendorDropdownRef = ref(null)
const isSaving = ref(false)
const lastTestOk = ref(false)
const backupPassphrase = ref('')
const isRefreshingBackups = ref(false)
const restoringName = ref('')
const deletingName = ref('')

const vendorOptions = computed(() => [
  { value: 'jianguoyun', label: t('webdavConfig.vendors.jianguoyun'), defaultUrl: 'https://dav.jianguoyun.com/dav/' },
  { value: 'self_hosted', label: t('webdavConfig.vendors.selfHosted'), defaultUrl: '' }
])

const backups = computed(() => settingsStore.webdavBackups || [])
const isBusy = computed(() => (
  settingsStore.isLoadingWebdav ||
  settingsStore.isTestingWebdav ||
  settingsStore.isBackingUpWebdav ||
  settingsStore.isRestoringWebdav ||
  settingsStore.isDeletingWebdavBackup ||
  isSaving.value
))

const vendorLabel = computed(() => {
  if (config.value.vendor === 'custom') {
    return t('webdavConfig.vendors.selfHosted')
  }
  return vendorOptions.value.find(option => option.value === config.value.vendor)?.label || config.value.vendor
})

const vendorHint = computed(() => {
  if (config.value.vendor === 'custom') {
    return t('webdavConfig.passwordHints.self_hosted')
  }
  return t(`webdavConfig.passwordHints.${config.value.vendor}`)
})

const passwordPlaceholder = computed(() => {
  return hasExistingPassword.value
    ? t('webdavConfig.placeholders.passwordKeep')
    : t('webdavConfig.placeholders.password')
})

const normalizedRetentionCount = computed(() => {
  const value = Number(config.value.retentionCount || 1)
  return Number.isFinite(value) ? Math.max(1, Math.min(30, Math.floor(value))) : 1
})

const canTest = computed(() => {
  return config.value.url.trim() &&
    config.value.username.trim() &&
    config.value.remoteDir.trim() &&
    (config.value.password || hasExistingPassword.value)
})

const canSave = computed(() => canTest.value)

const canBackup = computed(() => {
  return config.value.enabled &&
    hasExistingConfig.value &&
    backupPassphrase.value.trim() &&
    !settingsStore.isBackingUpWebdav &&
    !settingsStore.isLoadingWebdav
})

const buildRequest = () => ({
  enabled: config.value.enabled,
  vendor: config.value.vendor,
  url: config.value.url.trim(),
  username: config.value.username.trim(),
  password: config.value.password || null,
  remoteDir: config.value.remoteDir.trim() || 'ATM',
  retentionCount: normalizedRetentionCount.value
})

const toggleVendorMenu = () => {
  if (isBusy.value) return
  showVendorMenu.value = !showVendorMenu.value
}

const selectVendor = (option) => {
  config.value.vendor = option.value
  config.value.url = option.defaultUrl || ''
  showVendorMenu.value = false
}

const handleDocumentClick = (event) => {
  if (!showVendorMenu.value) return
  const dropdownEl = vendorDropdownRef.value
  if (dropdownEl && !dropdownEl.contains(event.target)) {
    showVendorMenu.value = false
  }
}

const loadConfig = async () => {
  try {
    const loaded = await settingsStore.loadWebdavConfig(true)
    config.value = {
      enabled: loaded.enabled || false,
      vendor: loaded.vendor || 'jianguoyun',
      url: loaded.url || (loaded.vendor === 'jianguoyun' ? 'https://dav.jianguoyun.com/dav/' : ''),
      username: loaded.username || '',
      password: '',
      remoteDir: loaded.remoteDir || 'ATM',
      retentionCount: loaded.retentionCount || 1
    }
    hasExistingPassword.value = Boolean(loaded.hasPassword)
    hasExistingConfig.value = Boolean(loaded.hasPassword || loaded.enabled || loaded.username)
    if (loaded.enabled) {
      refreshBackups({ silent: true })
    }
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.loadFailed')}: ${error}`)
  }
}

const testConnection = async () => {
  try {
    await settingsStore.testWebdavConnection(buildRequest())
    lastTestOk.value = true
    window.$notify?.success(t('webdavConfig.messages.testSuccess'))
  } catch (error) {
    lastTestOk.value = false
    window.$notify?.error(`${t('webdavConfig.messages.testFailed')}: ${error}`)
  }
}

const saveConfig = async () => {
  isSaving.value = true
  try {
    await settingsStore.saveWebdavConfig(buildRequest())
    hasExistingConfig.value = true
    hasExistingPassword.value = true
    config.value.password = ''
    window.$notify?.success(t('webdavConfig.messages.saveSuccess'))
    emit('saved')
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.saveFailed')}: ${error}`)
  } finally {
    isSaving.value = false
  }
}

const deleteConfig = async () => {
  const confirmed = await window.$confirm({
    title: t('webdavConfig.delete'),
    message: t('webdavConfig.messages.confirmDelete'),
    confirmText: t('webdavConfig.delete'),
    cancelText: t('common.cancel'),
    variant: 'danger'
  })
  if (!confirmed) return

  try {
    await settingsStore.deleteWebdavConfig()
    window.$notify?.success(t('webdavConfig.messages.deleteSuccess'))
    emit('deleted')
    emit('close')
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.deleteFailed')}: ${error}`)
  }
}

const backupNow = async () => {
  try {
    const result = await settingsStore.backupWebdavNow(backupPassphrase.value)
    window.$notify?.success(t('webdavConfig.messages.backupSuccess', { name: result.fileName }))
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.backupFailed')}: ${error}`)
  }
}

const refreshBackups = async (options = {}) => {
  isRefreshingBackups.value = true
  try {
    await settingsStore.listWebdavBackups()
  } catch (error) {
    if (!options.silent) {
      window.$notify?.error(`${t('webdavConfig.messages.listFailed')}: ${error}`)
    }
  } finally {
    isRefreshingBackups.value = false
  }
}

const restoreBackup = async (backup) => {
  if (!backupPassphrase.value.trim()) {
    window.$notify?.warning(t('webdavConfig.messages.passphraseRequired'))
    return
  }

  const confirmed = await window.$confirm({
    title: t('webdavConfig.restore'),
    message: t('webdavConfig.messages.confirmRestore', { name: backup.name }),
    confirmText: t('webdavConfig.restore'),
    cancelText: t('common.cancel'),
    variant: 'danger'
  })
  if (!confirmed) return

  restoringName.value = backup.name
  try {
    const result = await settingsStore.restoreWebdavBackup({
      fileName: backup.name,
      passphrase: backupPassphrase.value
    })
    window.$notify?.success(t('webdavConfig.messages.restoreSuccess', { count: result.restoredFiles }))
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.restoreFailed')}: ${error}`)
  } finally {
    restoringName.value = ''
  }
}

const deleteBackup = async (backup) => {
  const confirmed = await window.$confirm({
    title: t('webdavConfig.deleteBackup'),
    message: t('webdavConfig.messages.confirmDeleteBackup', { name: backup.name }),
    confirmText: t('webdavConfig.deleteBackup'),
    cancelText: t('common.cancel'),
    variant: 'danger'
  })
  if (!confirmed) return

  deletingName.value = backup.name
  try {
    await settingsStore.deleteWebdavBackup(backup.name)
    window.$notify?.success(t('webdavConfig.messages.deleteBackupSuccess', { name: backup.name }))
  } catch (error) {
    window.$notify?.error(`${t('webdavConfig.messages.deleteBackupFailed')}: ${error}`)
  } finally {
    deletingName.value = ''
  }
}

const formatBytes = (bytes) => {
  const value = Number(bytes || 0)
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

const formatDate = (value) => {
  if (!value) return '-'
  try {
    return new Date(value).toLocaleString()
  } catch {
    return value
  }
}

watch(config, () => {
  lastTestOk.value = false
}, { deep: true })

onMounted(() => {
  loadConfig()
  document.addEventListener('click', handleDocumentClick)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleDocumentClick)
})
</script>
