<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, Download, Upload } from '@lucide/vue'
import { NAlert, NButton, NInput, NPopconfirm, NSpace, NSwitch, useMessage } from 'naive-ui'
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
const providerLabel = computed(() => provider.value === 'github_gist' ? 'GitHub Gist' : 'Gitee 私有代码片段')
const configuredProviderLabel = computed(() => status.value?.provider === 'gitee_snippet' ? 'Gitee 私有代码片段' : 'GitHub Gist')



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
    if (formatted.includes('同步冲突')) {
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
    message.warning(`请输入 ${providerLabel.value} token。`)
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
    message.warning(discovery.value?.remoteExists ? '请输入云端同步密码。' : '请输入至少 8 个字符的同步密码。')
    return
  }
  if (!discovery.value?.remoteExists && syncPassword.value !== confirmSyncPassword.value) {
    message.warning('两次输入的同步密码不一致。')
    return
  }

  const command = provider.value === 'github_gist' ? 'enable_github_gist_sync' : 'enable_gitee_snippet_sync'
  const succeeded = await run(
    () => invoke<SyncStatus>(command, {
      token: token.value,
      syncPassword: syncPassword.value,
    }),
    discovery.value?.remoteExists
      ? `已验证云端同步密码并导入 ${providerLabel.value} 同步库。`
      : `已创建 ${providerLabel.value} 同步库，并已将本机凭据保存到系统凭据管理器。`,
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
    passwordError.value = '请输入当前云端同步密码。'
    return
  }

  loading.value = true
  try {
    applyStatus(await invoke<SyncStatus>('update_local_sync_password', {
      password: localSyncPassword.value,
    }))
    message.success('已更新本机同步凭据；云端和本地配置均未修改。')
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
    passwordError.value = '请输入当前密码和新密码。'
    return
  }
  if (newPassword.value !== confirmNewPassword.value) {
    passwordError.value = '两次输入的新同步密码不一致。'
    return
  }

  loading.value = true
  try {
    const result = await invoke<OperationResult>('change_sync_password', {
      currentPassword: currentPassword.value,
      newPassword: newPassword.value,
    })
    applyStatus(result.sync)
    message.success('已更新同步密码。所有同步设备请使用新密码。')
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
    resolution === 'keep_local' ? '已保留本地配置并覆盖远端；冲突前的两份数据已备份。' : '已采用远端配置；冲突前的两份数据已备份。',
    resolution === 'accept_remote',
  )
}

async function deleteRemote() {
  loading.value = true
  try {
    await invoke('delete_remote_sync_vault')
    await loadStatus()
    window.dispatchEvent(new Event('sync-configuration-changed'))
    message.success('已删除远端同步库及本机保存的同步凭据。')
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
    message.success('已解除本机同步绑定；远端 Gist 未删除。')
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
    <h3>云同步</h3>
    <n-alert type="info" :show-icon="false">
      同步密码仅用于端到端加密云端副本，不影响本地 SSH 凭证。密码不会上传，且无法找回。
    </n-alert>

    <div v-if="hasConflict && isConfigured" class="sync-card conflict-card">
      <div class="sync-card-title">同步冲突</div>
      <p>本地和远端自上次同步后都发生了变化。选择覆盖前会备份本地 Vault 与下载的远端加密文件到应用数据目录的 <code>sync-conflicts</code>。</p>
      <n-space>
        <n-button type="warning" :loading="loading" @click="resolveConflict('keep_local')">保留本地并覆盖远端</n-button>
        <n-button :loading="loading" @click="resolveConflict('accept_remote')">采用远端并覆盖本地</n-button>
        <n-button tertiary :disabled="loading" @click="conflictMessage = null">取消</n-button>
      </n-space>
    </div>

    <template v-if="!isConfigured">
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />配置云同步</div>
        <p>访问 token 仅用于检查云端同步库，探测成功前不会保存到系统凭据管理器。</p>
        <div class="setup-step">
          <strong>1. 连接云端</strong>
          <label>同步提供方
            <select v-model="provider" :disabled="loading || discovery !== null" @change="resetDiscovery">
              <option value="github_gist">GitHub Gist</option>
              <option value="gitee_snippet">Gitee 私有代码片段</option>
            </select>
          </label>
          <label>{{ providerLabel }} token<n-input v-model:value="token" type="password" show-password-on="click" :disabled="loading || discovery !== null" placeholder="仅在完成配置后保存到系统凭据管理器" /></label>
          <n-button v-if="!discovery" type="primary" :loading="loading" @click="discoverRemote">下一步</n-button>
        </div>

        <div v-if="discovery" class="setup-step">
          <strong>2. {{ discovery.remoteExists ? '验证云端同步密码' : '设置云端同步密码' }}</strong>
          <p v-if="discovery.remoteExists">已找到唯一的 MJJSSH 云端同步库。输入其同步密码后将验证并导入云端配置。</p>
          <p v-else>未找到云端同步库。设置密码后将创建一个新的加密同步库。</p>
          <label>{{ discovery.remoteExists ? '云端同步密码' : '新云同步密码' }}<n-input v-model:value="syncPassword" type="password" show-password-on="click" :disabled="loading" placeholder="至少 8 个字符" /></label>
          <label v-if="!discovery.remoteExists">确认云同步密码<n-input v-model:value="confirmSyncPassword" type="password" show-password-on="click" :disabled="loading" placeholder="再次输入同步密码" /></label>
          <n-space>
            <n-button :disabled="loading" @click="resetDiscovery">上一步</n-button>
            <n-button type="primary" :loading="loading" @click="enable">{{ discovery.remoteExists ? '验证并导入' : '设置密码并创建' }}</n-button>
          </n-space>
        </div>
      </div>
    </template>

    <template v-else>
      <div class="sync-card">
        <div class="sync-card-title"><Cloud :size="19" />{{ configuredProviderLabel }} 已配置</div>
        <p>同步文件：<code>{{ status?.remoteFileName }}</code></p>
        <p>访问 token 和派生同步密钥仅保存在系统凭据管理器中，不会返回给界面或写入 <code>sync.json</code>。</p>
        <p v-if="status?.lastSyncedAt">上次成功同步：{{ new Date(status.lastSyncedAt).toLocaleString() }}</p>
        <div class="sync-option">
          <div>
            <strong>自动同步</strong>
            <p>本地配置变更后等待 60 秒；连续修改会重新计时。</p>
          </div>
          <n-switch :value="status?.autoSync ?? true" :disabled="loading" @update:value="updateAutoSync" />
        </div>
        <div class="sync-option">
          <div>
            <strong>安全同步（推荐）</strong>
            <p>自动处理单侧更新；本地和云端同时变化时保留两份数据并提示选择。</p>
          </div>
        </div>


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
          <n-button tertiary :disabled="loading" @click="openLocalPasswordForm">更新本机同步密码</n-button>
        </n-space>
      </div>
      <div v-if="localPasswordFormVisible" class="sync-card password-form">
        <div class="sync-card-title">更新本机同步密码</div>
        <p>用于其他设备修改了云端同步密码后的重新连接。此操作只验证云端密码并更新本机凭据，不会上传、下载或修改任何配置。</p>
        <n-alert v-if="passwordError" type="error" :show-icon="false">{{ passwordError }}</n-alert>
        <label>当前云端同步密码<n-input v-model:value="localSyncPassword" type="password" show-password-on="click" placeholder="输入其他设备设置的新密码" /></label>
        <n-space>
          <n-button :disabled="loading" @click="closeLocalPasswordForm">取消</n-button>
          <n-button type="primary" :loading="loading" @click="updateLocalSyncPassword">仅更新本机</n-button>
        </n-space>
      </div>
      <div class="sync-card danger-zone">
        <div class="sync-card-title">危险区域</div>
        <p>这些操作会重新加密云端数据、解除本机同步绑定或永久删除云端数据，请谨慎操作。</p>
        <n-space>
        <n-button tertiary type="warning" :disabled="loading" @click="openPasswordForm">修改云端同步密码</n-button>
          <n-popconfirm
            :disabled="loading"
            positive-text="确认关闭"
            negative-text="取消"
            @positive-click="disable"
          >
            <template #trigger>
              <n-button tertiary type="warning" :disabled="loading">关闭云端同步</n-button>
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
              <n-button tertiary type="error" :disabled="loading">删除云端数据</n-button>
            </template>
            确定永久删除远端加密同步数据吗？<br>
            此操作不可恢复。
          </n-popconfirm>
        </n-space>
        <div v-if="passwordFormVisible" class="password-form">
          <div class="sync-card-title">修改云端同步密码</div>
          <p>此操作会使用本机配置重新加密并覆盖云端数据。请仅在确认本机配置是最新版本时使用。</p>
          <n-alert v-if="passwordError" type="error" :show-icon="false">{{ passwordError }}</n-alert>
          <label>当前同步密码<n-input v-model:value="currentPassword" type="password" show-password-on="click" placeholder="输入当前同步密码" /></label>
          <label>新同步密码<n-input v-model:value="newPassword" type="password" show-password-on="click" placeholder="至少 8 个字符" /></label>
          <label>确认新同步密码<n-input v-model:value="confirmNewPassword" type="password" show-password-on="click" placeholder="再次输入新同步密码" /></label>
          <n-space class="password-actions">
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
