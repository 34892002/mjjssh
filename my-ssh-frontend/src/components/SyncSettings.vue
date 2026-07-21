<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, Download, Upload } from '@lucide/vue'
import { NAlert, NButton, NInput, NPopconfirm, NSpace } from 'naive-ui'
import { useVaultStore } from '../stores/vault'

type SyncProvider = 'github_gist' | 'gitee_snippet'
type SyncStatus = {
  configured: boolean
  provider: string | null
  remoteId: string | null
  remoteFileName: string | null
  state: string
  lastSyncedAt: string | null
  deviceId: string | null
  token: string | null
  localVaultRevision: number | null
  lastSyncedVaultRevision: number | null
}

type OperationResult = {
  status: 'uploaded' | 'downloaded' | 'unchanged'
  sync: SyncStatus
}

const vaultStore = useVaultStore()
const status = ref<SyncStatus | null>(null)
const provider = ref<SyncProvider>('github_gist')
const token = ref('')
const syncPassword = ref('')
const confirmSyncPassword = ref('')
const currentPassword = ref('')
const newPassword = ref('')
const confirmNewPassword = ref('')
const passwordFormVisible = ref(false)
const passwordError = ref<string | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)

const isConfigured = computed(() => status.value?.configured === true)
const hasConflict = computed(() => error.value?.includes('同步冲突') === true)
const providerLabel = computed(() => provider.value === 'github_gist' ? 'GitHub Gist' : 'Gitee 私有代码片段')
const configuredProviderLabel = computed(() => status.value?.provider === 'gitee_snippet' ? 'Gitee 私有代码片段' : 'GitHub Gist')



function applyStatus(nextStatus: SyncStatus) {
  status.value = nextStatus
  if (nextStatus.provider === 'github_gist' || nextStatus.provider === 'gitee_snippet') {
    provider.value = nextStatus.provider
  }
  if (nextStatus.token) token.value = nextStatus.token
}

async function loadStatus() {
  try {
    applyStatus(await invoke<SyncStatus>('get_sync_status'))
  } catch (reason) {
    error.value = formatSyncError(reason)
  }
}

async function run(
  operation: () => Promise<SyncStatus | OperationResult>,
  success: string,
  refreshVault = false,
): Promise<boolean> {
  error.value = null
  notice.value = null
  loading.value = true
  try {
    const result = await operation()
    applyStatus('sync' in result ? result.sync : result)
    if (refreshVault) await vaultStore.refreshAfterSync()
    notice.value = success
    return true
  } catch (reason) {
    error.value = formatSyncError(reason)
    return false
  } finally {
    loading.value = false
  }
}

async function enable() {
  if (!token.value.trim() || !syncPassword.value) {
    error.value = '请输入访问 token 和至少 8 个字符的同步密码。'
    return
  }
  if (syncPassword.value !== confirmSyncPassword.value) {
    error.value = '两次输入的同步密码不一致。'
    return
  }
  const command = provider.value === 'github_gist' ? 'enable_github_gist_sync' : 'enable_gitee_snippet_sync'
  await run(
    () => invoke<SyncStatus>(command, {
      token: token.value,
      syncPassword: syncPassword.value,
    }),
    `已连接或创建 ${providerLabel.value} 同步库，并已在本机保存 token 和派生密钥。`,
    true,
  )
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
    return '同步冲突：本地或云端数据自上次同步后已发生变化。'
  }
  if (normalized.includes('authentication failed')) {
    return '云同步 token 无效、已过期或没有访问权限。'
  }
  if (normalized.includes('rate limit was reached')) {
    return '云同步服务请求过于频繁，请稍后重试。'
  }
  if (normalized.includes('gist was not found') || normalized.includes('snippet was not found')) {
    return '找不到云端同步数据，可能已被删除。'
  }
  if (normalized.includes('sync password is incorrect or sync data is corrupted')) {
    return '无法解密云端数据。同步密码可能被其他设备修改，或云端数据损坏。'
  }
  return message
}

function openPasswordForm() {
  passwordError.value = null
  passwordFormVisible.value = true
}

function closePasswordForm() {
  passwordFormVisible.value = false
  passwordError.value = null
  currentPassword.value = ''
  newPassword.value = ''
  confirmNewPassword.value = ''
}

async function changeSyncPassword() {
  passwordError.value = null
  if (!token.value.trim() || !currentPassword.value || !newPassword.value) {
    passwordError.value = '请输入当前密码和新密码。'
    return
  }
  if (newPassword.value !== confirmNewPassword.value) {
    passwordError.value = '两次输入的新同步密码不一致。'
    return
  }

  error.value = null
  notice.value = null
  loading.value = true
  try {
    const result = await invoke<OperationResult>('change_sync_password', {
      token: token.value,
      currentPassword: currentPassword.value,
      newPassword: newPassword.value,
    })
    applyStatus(result.sync)
    notice.value = '已更新同步密码。所有同步设备请使用新密码。'
    closePasswordForm()
  } catch (reason) {
    passwordError.value = formatSyncError(reason)
  } finally {
    loading.value = false
  }
}

async function resolveConflict(resolution: 'keep_local' | 'accept_remote') {
  if (!token.value.trim()) {
    error.value = '解决冲突需要访问 token。'
    return
  }
  await run(
    () => invoke<OperationResult>('resolve_sync_conflict', {
      token: token.value,
      resolution,
    }),
    resolution === 'keep_local' ? '已保留本地配置并覆盖远端；冲突前的两份数据已备份。' : '已采用远端配置；冲突前的两份数据已备份。',
    resolution === 'accept_remote',
  )
}

async function deleteRemote() {
  if (!token.value.trim()) {
    error.value = '删除远端同步库需要访问 token。'
    return
  }
  error.value = null
  notice.value = null
  loading.value = true
  try {
    await invoke('delete_remote_sync_vault', { token: token.value })
    await loadStatus()
    notice.value = '已删除远端同步库及本机保存的同步凭据。'
  } catch (reason) {
    error.value = formatSyncError(reason)
  } finally {
    loading.value = false
  }
}

async function disable() {
  error.value = null
  notice.value = null
  loading.value = true
  try {
    await invoke('disable_sync')
    await loadStatus()
    notice.value = '已解除本机同步绑定；远端 Gist 未删除。'
  } catch (reason) {
    error.value = formatSyncError(reason)
  } finally {
    loading.value = false
  }
}

onMounted(() => { void loadStatus() })
onBeforeUnmount(() => {
  token.value = ''
  syncPassword.value = ''
  confirmSyncPassword.value = ''
  currentPassword.value = ''
  newPassword.value = ''
  confirmNewPassword.value = ''
  passwordFormVisible.value = false
  passwordError.value = null
})
</script>

<template>
  <section class="sync-settings">
    <h3>云同步</h3>
    <n-alert type="info" :show-icon="false">
      同步密码仅用于端到端加密云端副本，不影响本地 SSH 凭证。密码不会上传，且无法找回。
    </n-alert>

    <n-alert v-if="error" type="error" :show-icon="false" class="sync-message">{{ error }}</n-alert>
    <n-alert v-if="notice" type="success" :show-icon="false" class="sync-message">{{ notice }}</n-alert>
    <div v-if="hasConflict && isConfigured" class="sync-card conflict-card">
      <div class="sync-card-title">同步冲突</div>
      <p>本地和远端自上次同步后都发生了变化。选择覆盖前会备份本地 Vault 与下载的远端加密文件到应用数据目录的 <code>sync-conflicts</code>。</p>
      <n-space>
        <n-button type="warning" :loading="loading" @click="resolveConflict('keep_local')">保留本地并覆盖远端</n-button>
        <n-button :loading="loading" @click="resolveConflict('accept_remote')">采用远端并覆盖本地</n-button>
        <n-button tertiary :disabled="loading" @click="error = null">取消</n-button>
      </n-space>
    </div>

    <template v-if="!isConfigured">
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />配置云同步</div>
        <p>应用会按名称自动查找唯一的 MJJSSH 私有同步片段：找到后自动导入远端数据，找不到才创建。token 和由同步密码派生的 AES 密钥会保存在本机 <code>sync.json</code>，原始密码不会保存。</p>
        <label>同步提供方
          <select v-model="provider">
            <option value="github_gist">GitHub Gist</option>
            <option value="gitee_snippet">Gitee 私有代码片段</option>
          </select>
        </label>

        <label>{{ providerLabel }} token<n-input v-model:value="token" type="password" show-password-on="click" placeholder="保存在本机 sync.json" /></label>
        <label>云同步加密密码<n-input v-model:value="syncPassword" type="password" show-password-on="click" placeholder="至少 8 个字符" /></label>
        <label>确认云同步加密密码<n-input v-model:value="confirmSyncPassword" type="password" show-password-on="click" placeholder="再次输入同步密码" /></label>
        <n-button type="primary" :loading="loading" @click="enable">连接并同步 {{ providerLabel }}</n-button>
      </div>
    </template>

    <template v-else>
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />{{ configuredProviderLabel }} 已配置</div>
        <p>同步文件：<code>{{ status?.remoteFileName }}</code></p>
        <p>此 token 用于访问 {{ configuredProviderLabel }} 中的同步(加密)数据。</p>
        <p v-if="status?.lastSyncedAt">上次成功同步：{{ new Date(status.lastSyncedAt).toLocaleString() }}</p>
        <label>{{ configuredProviderLabel }} token<n-input v-model:value="token" type="password" show-password-on="click" placeholder="保存在本机 sync.json" /></label>

        <n-space>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认覆盖云端"
            negative-text="取消"
            @positive-click="overwriteWithLocal"
          >
            <template #trigger>
              <n-button type="primary" :loading="loading"><Upload :size="16" />本地覆盖云端</n-button>
            </template>
            将用本地配置覆盖云端同步数据。<br>
            覆盖前会自动备份本地和云端数据。
          </n-popconfirm>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认覆盖本地"
            negative-text="取消"
            @positive-click="overwriteWithRemote"
          >
            <template #trigger>
              <n-button :loading="loading"><Download :size="16" />云端覆盖本地</n-button>
            </template>
            将用云端配置覆盖本地数据。<br>
            覆盖前会自动备份本地和云端数据。
          </n-popconfirm>
          <n-button tertiary :disabled="loading" @click="openPasswordForm">修改同步密码</n-button>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认关闭"
            negative-text="取消"
            @positive-click="disable"
          >
            <template #trigger>
              <n-button tertiary type="warning" :disabled="loading">关闭同步</n-button>
            </template>
            将解除本机与云端同步的绑定。<br>
            不会删除云端同步数据。
          </n-popconfirm>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认删除"
            negative-text="取消"
            @positive-click="deleteRemote"
          >
            <template #trigger>
              <n-button tertiary type="error" :disabled="loading">删除远端数据</n-button>
            </template>
            确定永久删除远端加密同步数据吗？<br>
            此操作不可恢复。
          </n-popconfirm>
        </n-space>
      </div>
      <div v-if="passwordFormVisible" class="sync-card password-form">
        <div class="sync-card-title">修改同步密码</div>
        <p>所有同步此云端数据的设备都必须使用同一个同步密码。</p>
        <n-alert v-if="passwordError" type="error" :show-icon="false">{{ passwordError }}</n-alert>
        <label>当前同步密码<n-input v-model:value="currentPassword" type="password" show-password-on="click" placeholder="输入当前同步密码" /></label>
        <label>新同步密码<n-input v-model:value="newPassword" type="password" show-password-on="click" placeholder="至少 8 个字符" /></label>
        <label>确认新同步密码<n-input v-model:value="confirmNewPassword" type="password" show-password-on="click" placeholder="再次输入新同步密码" /></label>
        <n-space>
          <n-button :disabled="loading" @click="closePasswordForm">取消</n-button>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认更新并同步"
            negative-text="取消"
            @positive-click="changeSyncPassword"
          >
            <template #trigger>
              <n-button type="primary" :loading="loading">更新并同步</n-button>
            </template>
            将使用新密码生成新的加密同步配置，并覆盖云端现有同步数据。<br>
            所有其他设备之后都需要使用新密码才能继续同步。
          </n-popconfirm>
        </n-space>
      </div>

    </template>
  </section>
</template>

<style scoped>
.sync-settings { display: grid; gap: 16px; }
h3 { margin: 0; font-size: 18px; }
.sync-message { margin: 0; }
.sync-card { display: grid; gap: 13px; padding: 16px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-panel); }
.sync-card-title { display: flex; align-items: center; gap: 8px; font-weight: 650; }
p { margin: 0; color: var(--app-muted); font-size: 13px; line-height: 1.55; }
label { display: grid; gap: 6px; color: var(--app-text); font-size: 13px; font-weight: 600; }

select { width: 100%; padding: 8px 10px; border: 1px solid var(--app-border); border-radius: 6px; color: var(--app-text); background: var(--app-surface); }
code { font-size: 12px; word-break: break-all; }
</style>
