<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, Download, Upload } from '@lucide/vue'
import { NAlert, NButton, NInput, NPopconfirm, NSpace, NSwitch, useMessage } from 'naive-ui'
import { useVaultStore } from '../stores/vault'
import { useLocale } from '../composables/useLocale'

type SyncProvider = 'github_gist' | 'gitee_snippet'
type SyncStatus = {
  configured: boolean
  provider: string | null
  remoteId: string | null
  remoteFileName: string | null
  state: string
  lastSyncedAt: string | null
  deviceId: string | null
  autoSync: boolean
  localVaultRevision: number | null
  lastSyncedVaultRevision: number | null
}

type OperationResult = {
  status: 'uploaded' | 'downloaded' | 'unchanged'
  sync: SyncStatus
}

type SyncDiscovery = {
  remoteExists: boolean
}

const vaultStore = useVaultStore()
const { t } = useLocale()
const message = useMessage()
const status = ref<SyncStatus | null>(null)
const provider = ref<SyncProvider>('github_gist')
const token = ref('')
const syncPassword = ref('')
const confirmSyncPassword = ref('')
const discovery = ref<SyncDiscovery | null>(null)
const currentPassword = ref('')
const newPassword = ref('')
const confirmNewPassword = ref('')
const localSyncPassword = ref('')
const passwordFormVisible = ref(false)
const localPasswordFormVisible = ref(false)
const passwordError = ref<string | null>(null)
const loading = ref(false)
const conflictMessage = ref<string | null>(null)

const isConfigured = computed(() => status.value?.configured === true)
const hasConflict = computed(() => conflictMessage.value !== null)
const providerLabel = computed(() => provider.value === 'github_gist' ? 'GitHub Gist' : t('sync.giteePrivateSnippet'))
const configuredProviderLabel = computed(() => status.value?.provider === 'gitee_snippet' ? t('sync.giteePrivateSnippet') : 'GitHub Gist')



function applyStatus(nextStatus: SyncStatus) {
  status.value = nextStatus
  if (nextStatus.provider === 'github_gist' || nextStatus.provider === 'gitee_snippet') {
    provider.value = nextStatus.provider
  }

}

async function loadStatus() {
  try {
    applyStatus(await invoke<SyncStatus>('get_sync_status'))
  } catch (reason) {
    message.error(formatSyncError(reason))
  }
}

async function run(
  operation: () => Promise<SyncStatus | OperationResult>,
  success: string,
  refreshVault = false,
): Promise<boolean> {
  conflictMessage.value = null
  loading.value = true
  try {
    const result = await operation()
    applyStatus('sync' in result ? result.sync : result)
    if (refreshVault) await vaultStore.refreshAfterSync()
    message.success(success)
    return true
  } catch (reason) {
    const formatted = formatSyncError(reason)
    if (formatted.includes(t('sync.conflictPrefix'))) {
      conflictMessage.value = formatted
      message.warning(formatted, { keepAliveOnHover: true })
    } else {
      message.error(formatted)
    }
    return false
  } finally {
    loading.value = false
  }
}

function resetDiscovery() {
  discovery.value = null
  syncPassword.value = ''
  confirmSyncPassword.value = ''
}

async function discoverRemote() {
  if (!token.value.trim()) {
    message.warning(t('sync.tokenRequired', { provider: providerLabel.value }))
    return
  }

  conflictMessage.value = null
  loading.value = true
  try {
    discovery.value = await invoke<SyncDiscovery>('discover_sync_remote', {
      provider: provider.value,
      token: token.value,
    })
  } catch (reason) {
    message.error(formatSyncError(reason))
  } finally {
    loading.value = false
  }
}

async function enable() {
  if (!syncPassword.value) {
    message.warning(discovery.value?.remoteExists ? t('sync.remotePasswordRequired') : t('sync.minimumPasswordRequired'))
    return
  }
  if (!discovery.value?.remoteExists && syncPassword.value !== confirmSyncPassword.value) {
    message.warning(t('sync.passwordMismatch'))
    return
  }

  const command = provider.value === 'github_gist' ? 'enable_github_gist_sync' : 'enable_gitee_snippet_sync'
  const succeeded = await run(
    () => invoke<SyncStatus>(command, {
      token: token.value,
      syncPassword: syncPassword.value,
    }),
    discovery.value?.remoteExists
      ? t('sync.remotePasswordVerifiedAndImported', { provider: providerLabel.value })
      : t('sync.syncVaultCreated', { provider: providerLabel.value }),
    true,
  )
  if (succeeded) {
    token.value = ''
    resetDiscovery()
    window.dispatchEvent(new Event('sync-configuration-changed'))
  }
}

async function overwriteWithLocal() {
  await resolveConflict('keep_local')
}

async function overwriteWithRemote() {
  await resolveConflict('accept_remote')
}

function formatSyncError(reason: unknown): string {
  const message = String(reason)
  const normalized = message.toLowerCase()
  if (normalized.includes('cloud sync conflict') || normalized.includes('rejected the update because the remote changed')) {
    return t('sync.conflictError')
  }
  if (normalized.includes('authentication failed')) {
    return t('sync.authenticationFailed')
  }
  if (normalized.includes('rate limit was reached')) {
    return t('sync.rateLimited')
  }
  if (normalized.includes('gist was not found') || normalized.includes('snippet was not found')) {
    return t('sync.remoteNotFound')
  }
  if (normalized.includes('sync password is incorrect or sync data is corrupted')) {
    return t('sync.decryptionFailed')
  }
  return message
}

function openPasswordForm() {
  passwordError.value = null
  localPasswordFormVisible.value = false
  passwordFormVisible.value = true
}

function closePasswordForm() {
  passwordFormVisible.value = false
  passwordError.value = null
  currentPassword.value = ''
  newPassword.value = ''
  confirmNewPassword.value = ''
}

function openLocalPasswordForm() {
  passwordError.value = null
  passwordFormVisible.value = false
  localPasswordFormVisible.value = true
}

function closeLocalPasswordForm() {
  localPasswordFormVisible.value = false
  passwordError.value = null
  localSyncPassword.value = ''
}

async function updateLocalSyncPassword() {
  passwordError.value = null
  if (!localSyncPassword.value) {
    passwordError.value = t('sync.currentRemotePasswordRequired')
    return
  }

  loading.value = true
  try {
    applyStatus(await invoke<SyncStatus>('update_local_sync_password', {
      password: localSyncPassword.value,
    }))
    message.success(t('sync.localCredentialsUpdated'))
    closeLocalPasswordForm()
  } catch (reason) {
    passwordError.value = formatSyncError(reason)
  } finally {
    loading.value = false
  }
}

async function updateAutoSync(autoSync: boolean) {
  const previous = status.value?.autoSync ?? true
  if (status.value) status.value.autoSync = autoSync
  try {
    applyStatus(await invoke<SyncStatus>('set_auto_sync', { autoSync }))
    window.dispatchEvent(new Event('sync-configuration-changed'))
  } catch (reason) {
    if (status.value) status.value.autoSync = previous
    message.error(formatSyncError(reason))
  }
}

async function changeSyncPassword() {
  passwordError.value = null
  if (!currentPassword.value || !newPassword.value) {
    passwordError.value = t('sync.currentAndNewPasswordRequired')
    return
  }
  if (newPassword.value !== confirmNewPassword.value) {
    passwordError.value = t('sync.newPasswordMismatch')
    return
  }

  loading.value = true
  try {
    const result = await invoke<OperationResult>('change_sync_password', {
      currentPassword: currentPassword.value,
      newPassword: newPassword.value,
    })
    applyStatus(result.sync)
    message.success(t('sync.passwordUpdated'))
    closePasswordForm()
  } catch (reason) {
    passwordError.value = formatSyncError(reason)
  } finally {
    loading.value = false
  }
}

async function resolveConflict(resolution: 'keep_local' | 'accept_remote') {

  await run(
    () => invoke<OperationResult>('resolve_sync_conflict', {
      resolution,
    }),
    resolution === 'keep_local' ? t('sync.keptLocalAndBackedUp') : t('sync.acceptedRemoteAndBackedUp'),
    resolution === 'accept_remote',
  )
}

async function deleteRemote() {
  loading.value = true
  try {
    await invoke('delete_remote_sync_vault')
    await loadStatus()
    window.dispatchEvent(new Event('sync-configuration-changed'))
    message.success(t('sync.remoteVaultDeleted'))
  } catch (reason) {
    message.error(formatSyncError(reason))
  } finally {
    loading.value = false
  }
}

async function disable() {
  loading.value = true
  try {
    await invoke('disable_sync')
    await loadStatus()
    window.dispatchEvent(new Event('sync-configuration-changed'))
    message.success(t('sync.syncDisabled'))
  } catch (reason) {
    message.error(formatSyncError(reason))
  } finally {
    loading.value = false
  }
}

onMounted(() => { void loadStatus() })
onBeforeUnmount(() => {
  token.value = ''
  syncPassword.value = ''
  confirmSyncPassword.value = ''
  discovery.value = null
  currentPassword.value = ''
  newPassword.value = ''
  confirmNewPassword.value = ''
  localSyncPassword.value = ''
  passwordFormVisible.value = false
  localPasswordFormVisible.value = false
  passwordError.value = null
})
</script>

<template>
  <section class="sync-settings">
    <h3>{{ t('sync.title') }}</h3>
    <n-alert type="info" :show-icon="false">
      {{ t('sync.passwordNotice') }}
    </n-alert>

    <div v-if="hasConflict && isConfigured" class="sync-card conflict-card">
      <div class="sync-card-title">{{ t('sync.conflict') }}</div>
      <p>{{ t('sync.conflictDescription') }} <code>sync-conflicts</code>.</p>
      <n-space>
        <n-button type="warning" :loading="loading" @click="resolveConflict('keep_local')">{{ t('sync.keepLocalOverwriteRemote') }}</n-button>
        <n-button :loading="loading" @click="resolveConflict('accept_remote')">{{ t('sync.acceptRemoteOverwriteLocal') }}</n-button>
        <n-button tertiary :disabled="loading" @click="conflictMessage = null">{{ t('sync.cancel') }}</n-button>
      </n-space>
    </div>

    <template v-if="!isConfigured">
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />{{ t('sync.configure') }}</div>
        <p>{{ t('sync.tokenNotice') }}</p>
        <div class="setup-step">
          <strong>{{ t('sync.connectRemoteStep') }}</strong>
          <label>{{ t('sync.provider') }}
            <select v-model="provider" :disabled="loading || discovery !== null" @change="resetDiscovery">
              <option value="github_gist">GitHub Gist</option>
              <option value="gitee_snippet">{{ t('sync.giteePrivateSnippet') }}</option>
            </select>
          </label>
          <label>{{ t('sync.tokenLabel', { provider: providerLabel }) }}<n-input v-model:value="token" type="password" show-password-on="click" :disabled="loading || discovery !== null" :placeholder="t('sync.tokenPlaceholder')" /></label>
          <n-button v-if="!discovery" type="primary" :loading="loading" @click="discoverRemote">{{ t('sync.next') }}</n-button>
        </div>

        <div v-if="discovery" class="setup-step">
          <strong>{{ discovery.remoteExists ? t('sync.verifyRemotePasswordStep') : t('sync.setRemotePasswordStep') }}</strong>
          <p v-if="discovery.remoteExists">{{ t('sync.remoteVaultFound') }}</p>
          <p v-else>{{ t('sync.remoteVaultNotFound') }}</p>
          <label>{{ discovery.remoteExists ? t('sync.remotePassword') : t('sync.newRemotePassword') }}<n-input v-model:value="syncPassword" type="password" show-password-on="click" :disabled="loading" :placeholder="t('sync.minimumPasswordPlaceholder')" /></label>
          <label v-if="!discovery.remoteExists">{{ t('sync.confirmRemotePassword') }}<n-input v-model:value="confirmSyncPassword" type="password" show-password-on="click" :disabled="loading" :placeholder="t('sync.confirmPasswordPlaceholder')" /></label>
          <n-space>
            <n-button :disabled="loading" @click="resetDiscovery">{{ t('sync.previous') }}</n-button>
            <n-button type="primary" :loading="loading" @click="enable">{{ discovery.remoteExists ? t('sync.verifyAndImport') : t('sync.setPasswordAndCreate') }}</n-button>
          </n-space>
        </div>
      </div>
    </template>

    <template v-else>
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />{{ t('sync.providerConfigured', { provider: configuredProviderLabel }) }}</div>
        <p>{{ t('sync.syncFile') }}: <code>{{ status?.remoteFileName }}</code></p>
        <p>{{ t('sync.credentialsNotice') }} <code>sync.json</code>.</p>
        <p v-if="status?.lastSyncedAt">{{ t('sync.lastSuccessfulSync') }}: {{ new Date(status.lastSyncedAt).toLocaleString() }}</p>
        <div class="sync-option">
          <div>
            <strong>{{ t('sync.autoSync') }}</strong>
            <p>{{ t('sync.autoSyncDescription') }}</p>
          </div>
          <n-switch :value="status?.autoSync ?? true" :disabled="loading" @update:value="updateAutoSync" />
        </div>
        <div class="sync-option">
          <div>
            <strong>{{ t('sync.safeSyncRecommended') }}</strong>
            <p>{{ t('sync.safeSyncDescription') }}</p>
          </div>
        </div>


        <n-space>
          <n-popconfirm
            :disabled="loading"
            :positive-text="t('sync.confirmOverwriteRemote')"
            :negative-text="t('sync.cancel')"
            @positive-click="overwriteWithLocal"
          >
            <template #trigger>
              <n-button type="primary" :loading="loading"><Upload :size="16" />{{ t('sync.localOverwriteRemote') }}</n-button>
            </template>
            {{ t('sync.localOverwriteWarning') }}<br>
            {{ t('sync.backupBeforeOverwrite') }}
          </n-popconfirm>
          <n-popconfirm
            :disabled="loading"
            :positive-text="t('sync.confirmOverwriteLocal')"
            :negative-text="t('sync.cancel')"
            @positive-click="overwriteWithRemote"
          >
            <template #trigger>
              <n-button :loading="loading"><Download :size="16" />{{ t('sync.remoteOverwriteLocal') }}</n-button>
            </template>
            {{ t('sync.remoteOverwriteWarning') }}<br>
            {{ t('sync.backupBeforeOverwrite') }}
          </n-popconfirm>
          <n-button tertiary :disabled="loading" @click="openLocalPasswordForm">{{ t('sync.updateLocalPassword') }}</n-button>
        </n-space>
      </div>
      <div v-if="localPasswordFormVisible" class="sync-card password-form">
        <div class="sync-card-title">{{ t('sync.updateLocalPassword') }}</div>
        <p>{{ t('sync.updateLocalPasswordDescription') }}</p>
        <n-alert v-if="passwordError" type="error" :show-icon="false">{{ passwordError }}</n-alert>
        <label>{{ t('sync.currentRemotePassword') }}<n-input v-model:value="localSyncPassword" type="password" show-password-on="click" :placeholder="t('sync.remotePasswordFromOtherDevicePlaceholder')" /></label>
        <n-space>
          <n-button :disabled="loading" @click="closeLocalPasswordForm">{{ t('sync.cancel') }}</n-button>
          <n-button type="primary" :loading="loading" @click="updateLocalSyncPassword">{{ t('sync.updateLocalOnly') }}</n-button>
        </n-space>
      </div>
      <div class="sync-card danger-zone">
        <div class="sync-card-title">{{ t('sync.dangerZone') }}</div>
        <p>{{ t('sync.dangerZoneDescription') }}</p>
        <n-space>
        <n-button tertiary type="warning" :disabled="loading" @click="openPasswordForm">{{ t('sync.changeRemotePassword') }}</n-button>
          <n-popconfirm
            :disabled="loading"
            :positive-text="t('sync.confirmDisable')"
            :negative-text="t('sync.cancel')"
            @positive-click="disable"
          >
            <template #trigger>
              <n-button tertiary type="warning" :disabled="loading">{{ t('sync.disable') }}</n-button>
            </template>
            {{ t('sync.disableWarning') }}<br>
            {{ t('sync.disableNotice') }}
          </n-popconfirm>
          <n-popconfirm
            :disabled="loading"
            :positive-text="t('sync.confirmDelete')"
            :negative-text="t('sync.cancel')"
            @positive-click="deleteRemote"
          >
            <template #trigger>
              <n-button tertiary type="error" :disabled="loading">{{ t('sync.deleteRemoteData') }}</n-button>
            </template>
            {{ t('sync.deleteRemoteDataWarning') }}<br>
            {{ t('sync.irreversible') }}
          </n-popconfirm>
        </n-space>
        <div v-if="passwordFormVisible" class="password-form">
          <div class="sync-card-title">{{ t('sync.changeRemotePassword') }}</div>
          <p>{{ t('sync.changePasswordDescription') }}</p>
          <n-alert v-if="passwordError" type="error" :show-icon="false">{{ passwordError }}</n-alert>
          <label>{{ t('sync.currentPassword') }}<n-input v-model:value="currentPassword" type="password" show-password-on="click" :placeholder="t('sync.currentPasswordPlaceholder')" /></label>
          <label>{{ t('sync.newPassword') }}<n-input v-model:value="newPassword" type="password" show-password-on="click" :placeholder="t('sync.minimumPasswordPlaceholder')" /></label>
          <label>{{ t('sync.confirmNewPassword') }}<n-input v-model:value="confirmNewPassword" type="password" show-password-on="click" :placeholder="t('sync.confirmNewPasswordPlaceholder')" /></label>
          <n-space class="password-actions">
            <n-button :disabled="loading" @click="closePasswordForm">{{ t('sync.cancel') }}</n-button>
            <n-popconfirm
              :disabled="loading"
              :positive-text="t('sync.confirmUpdateAndSync')"
              :negative-text="t('sync.cancel')"
              @positive-click="changeSyncPassword"
            >
              <template #trigger>
                <n-button type="primary" :loading="loading">{{ t('sync.updateAndSync') }}</n-button>
              </template>
              {{ t('sync.updateAndSyncWarning') }}<br>
              {{ t('sync.otherDevicesRequireNewPassword') }}
            </n-popconfirm>
          </n-space>
        </div>
      </div>

    </template>
  </section>
</template>

<style scoped>
.sync-settings { display: grid; gap: 16px; }
h3 { margin: 0; font-size: 18px; }

.sync-card { display: grid; gap: 13px; padding: 16px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-panel); }
.sync-card-title { display: flex; align-items: center; gap: 8px; font-weight: 650; }
p { margin: 0; color: var(--app-muted); font-size: 13px; line-height: 1.55; }
label { display: grid; gap: 6px; color: var(--app-text); font-size: 13px; font-weight: 600; }
.setup-step { display: grid; gap: 12px; padding: 13px; border: 1px solid var(--app-border); border-radius: 7px; }
.setup-step > strong { font-size: 13px; }

select { width: 100%; padding: 8px 10px; border: 1px solid var(--app-border); border-radius: 6px; color: var(--app-text); background: var(--app-surface); }
code { font-size: 12px; word-break: break-all; }
.sync-option { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 11px 12px; border: 1px solid var(--app-border); border-radius: 7px; }
.sync-option > div { display: grid; gap: 3px; }
.sync-option strong { font-size: 13px; }
.sync-option p { font-size: 12px; }
.danger-zone { border-color: color-mix(in srgb, var(--app-border), #ef4444 35%); }
.danger-zone .sync-card-title { color: #fca5a5; }
.password-actions { margin-top: 8px; }
</style>
