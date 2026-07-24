<script setup lang="ts">
import { defineAsyncComponent, ref, computed, onBeforeUnmount, onMounted, nextTick, watch, type Component } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Archive, Box, Cloud, CloudCog, Code2, Container, Copy, Cpu, Database, Download, EthernetPort, FileCode2, Globe2, HardDrive, Languages, Layers3, ListFilter, MapPin, MemoryStick, MonitorCog, Moon, Network, RadioTower, RefreshCw, Router, Server, ServerCog, Settings, ShieldCheck, Sparkles, Square, Sun, TerminalSquare, Upload, Waypoints, Workflow, X, Zap } from '@lucide/vue'
import {
  darkTheme,
  NConfigProvider,
  NMessageProvider,
  NButton,
  NAlert,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSpace,
  NPopconfirm,
  NPopover,
  NDropdown,
  NEmpty,
  NGlobalStyle,
  type GlobalThemeOverrides,
} from 'naive-ui'
import { useVaultStore } from './stores/vault'
import { useSessionStore } from './stores/session'
import { useTransferStore } from './stores/transfer'
import { useLocale, type AppLanguage } from './composables/useLocale'
import EntityCard from './components/EntityCard.vue'
const Terminal = defineAsyncComponent(() => import('./components/Terminal.vue'))
const ConnectionDialog = defineAsyncComponent(() => import('./components/ConnectionDialog.vue'))
const KeysView = defineAsyncComponent(() => import('./components/KeysView.vue'))
const ScriptsView = defineAsyncComponent(() => import('./components/ScriptsView.vue'))
const SftpView = defineAsyncComponent(() => import('./components/SftpView.vue'))
const AiChatPanel = defineAsyncComponent(() => import('./components/AiChatPanel.vue'))
const AiSettings = defineAsyncComponent(() => import('./components/AiSettings.vue'))
const SyncSettings = defineAsyncComponent(() => import('./components/SyncSettings.vue'))
const TransferPanel = defineAsyncComponent(() => import('./components/TransferPanel.vue'))
const ScriptPanel = defineAsyncComponent(() => import('./components/ScriptPanel.vue'))
const PermissionsDialog = defineAsyncComponent(() => import('./components/PermissionsDialog.vue'))
const ActionDialog = defineAsyncComponent(() => import('./components/ActionDialog.vue'))
import type { SshProfileView, CreateProfileRequest } from './types'

const vaultStore = useVaultStore()
const sessionStore = useSessionStore()
const transferStore = useTransferStore()
const appWindow = getCurrentWindow()
const { language, languageLabel, naiveLocale, naiveDateLocale, setLanguage, t } = useLocale()
const languageOptions = computed(() => [
  { label: t('language.zh'), key: 'zh-CN' },
  { label: t('language.en'), key: 'en-US' },
])
const dateLocale = computed(() => language.value === 'zh-CN' ? 'zh-CN' : 'en-US')

function formatDate(value: string) {
  return new Date(value).toLocaleString(dateLocale.value)
}
const savedTheme = localStorage.getItem('my-ssh-theme')
const isDarkTheme = ref(savedTheme !== 'light')
const naiveTheme = computed(() => isDarkTheme.value ? darkTheme : null)

const lightThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#2563eb',
    primaryColorHover: '#3b82f6',
    primaryColorPressed: '#1d4ed8',
    bodyColor: '#f8fafc',
    cardColor: '#ffffff',
    modalColor: '#ffffff',
    popoverColor: '#ffffff',
    hoverColor: '#f1f5f9',
    pressedColor: '#e2e8f0',
    borderColor: '#dbe3ef',
    dividerColor: '#e2e8f0',
    boxShadow2: '0 8px 18px rgba(15, 23, 42, .14)',
  },
}

const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#89b4fa',
    primaryColorHover: '#b4befe',
    primaryColorPressed: '#74c7ec',
    bodyColor: '#1e1e2e',
    cardColor: '#181825',
    modalColor: '#181825',
    popoverColor: '#1c2330',
    hoverColor: '#252e3e',
    pressedColor: '#2b3649',
    borderColor: '#344057',
    dividerColor: '#2d3748',
    boxShadow2: '0 8px 18px rgba(0, 0, 0, .32)',
  },
}

const naiveThemeOverrides = computed(() => isDarkTheme.value ? darkThemeOverrides : lightThemeOverrides)
type SyncStatus = {
  configured: boolean
  provider: string | null
  remoteId: string | null
  remoteFileName: string | null
  state: string
  lastSyncedAt: string | null
  deviceId: string | null
  token: string | null
  autoSync: boolean
  localVaultRevision: number | null
  lastSyncedVaultRevision: number | null
}

type SyncOperationResult = {
  status: 'uploaded' | 'downloaded' | 'unchanged'
  sync: SyncStatus
}

type RemoteSyncStatus = {
  state: 'in_sync' | 'local_ahead' | 'remote_ahead' | 'conflict'
  localVaultRevision: number
  remoteVaultRevision: number
  lastSyncedVaultRevision: number
  remoteUpdatedAt: string
}

const isMaximized = ref(false)
const syncPopoverVisible = ref(false)
const syncStatus = ref<SyncStatus | null>(null)
const syncLoading = ref(false)
const syncError = ref<string | null>(null)
const syncNotice = ref<string | null>(null)
const remoteSyncStatus = ref<RemoteSyncStatus | null>(null)
const remoteSyncLoading = ref(false)
const autoSyncState = ref<'idle' | 'pending' | 'syncing' | 'success' | 'conflict' | 'error'>('idle')
const autoSyncDueAt = ref<number | null>(null)
const autoSyncNow = ref(Date.now())
let autoSyncTimer: ReturnType<typeof setTimeout> | null = null
let autoSyncCountdownTimer: ReturnType<typeof setInterval> | null = null
let vaultMd5: string | null = null
let vaultMd5Timer: ReturnType<typeof setInterval> | null = null
const transferPanelRef = ref<HTMLElement | null>(null)
const transferButtonRef = ref<HTMLButtonElement | null>(null)
const scriptPanelRef = ref<HTMLElement | null>(null)
const scriptButtonRef = ref<HTMLButtonElement | null>(null)
const transferNoticeVisible = ref(false)
let transferNoticeTimer: ReturnType<typeof setTimeout> | null = null

const syncProviderLabel = computed(() => syncStatus.value?.provider === 'gitee_snippet' ? 'Gitee 私有代码片段' : 'GitHub Gist')
const syncVersionState = computed(() => {
  const state = remoteSyncStatus.value?.state
  if (!state) return t('sync.notCheckedRemote')
  return {
    in_sync: t('sync.inSync'),
    local_ahead: t('sync.localAhead'),
    remote_ahead: t('sync.remoteAhead'),
    conflict: t('sync.conflict'),
  }[state]
})

const remoteSyncAction = computed(() => {
  const state = remoteSyncStatus.value?.state
  if (!state) return ''
  return {
    in_sync: '',
    local_ahead: t('sync.uploadSuggestion'),
    remote_ahead: t('sync.downloadSuggestion'),
    conflict: t('sync.conflictSuggestion'),
  }[state]
})

function formatQuickSyncError(reason: unknown): string {
  const message = String(reason)
  const normalized = message.toLowerCase()
  if (normalized.includes('cloud sync conflict') || normalized.includes('rejected the update because the remote changed')) return '同步冲突：本地和云端都发生了变化，请在同步设置中选择保留哪一份数据。'
  if (normalized.includes('authentication failed')) return '云同步 token 无效、已过期或没有访问权限。'
  if (normalized.includes('rate limit was reached')) return '云同步服务请求过于频繁，请稍后重试。'
  if (normalized.includes('gist was not found') || normalized.includes('snippet was not found')) return '找不到云端同步数据，可能已被删除。'
  if (normalized.includes('sync password is incorrect or sync data is corrupted')) return '无法解密云端数据。同步密码可能被其他设备修改，或云端数据损坏。'
  return message
}

async function loadSyncStatus() {
  try {
    syncStatus.value = await invoke<SyncStatus>('get_sync_status')
  } catch (reason) {
    syncError.value = formatQuickSyncError(reason)
  }
}

async function checkRemoteSyncStatus() {
  if (!syncStatus.value?.token || remoteSyncLoading.value) return
  remoteSyncLoading.value = true
  try {
    remoteSyncStatus.value = await invoke<RemoteSyncStatus>('check_remote_sync_status', { token: syncStatus.value.token })
  } catch (reason) {
    remoteSyncStatus.value = null
    syncError.value = formatQuickSyncError(reason)
  } finally {
    remoteSyncLoading.value = false
  }
}

async function handleSyncPopoverShow(visible: boolean) {
  syncPopoverVisible.value = visible
  if (!visible) return
  if (['success', 'conflict', 'error'].includes(autoSyncState.value)) {
    autoSyncState.value = 'idle'
  }
  syncError.value = null
  syncNotice.value = null
  await loadSyncStatus()
  await checkRemoteSyncStatus()
}

async function syncNow(automatic = false) {
  if (!syncStatus.value?.token || syncLoading.value) return
  syncError.value = null
  syncNotice.value = null
  syncLoading.value = true
  if (automatic) {
    autoSyncDueAt.value = null
    autoSyncState.value = 'syncing'
  }
  try {
    const download = await invoke<SyncOperationResult>('download_sync_vault', { token: syncStatus.value.token })
    syncStatus.value = download.sync
    if (download.status === 'downloaded') {
      await vaultStore.refreshAfterSync()
      syncNotice.value = t('sync.downloaded')
    } else {
      const upload = await invoke<SyncOperationResult>('upload_sync_vault', { token: syncStatus.value.token })
      syncStatus.value = upload.sync
      syncNotice.value = upload.status === 'uploaded' ? t('sync.uploaded') : t('sync.upToDate')
    }
    if (automatic) autoSyncState.value = 'success'
    vaultMd5 = await readVaultMd5()
    await loadSyncStatus()
    await checkRemoteSyncStatus()
  } catch (reason) {
    syncError.value = formatQuickSyncError(reason)
    if (automatic) autoSyncState.value = syncError.value.includes('同步冲突') ? 'conflict' : 'error'
  } finally {
    syncLoading.value = false
  }
}

async function readVaultMd5(): Promise<string | null> {
  try {
    return await invoke<string>('get_vault_md5')
  } catch {
    return null
  }
}

async function checkVaultMd5() {
  const nextMd5 = await readVaultMd5()
  if (!nextMd5) return
  if (vaultMd5 === null) {
    vaultMd5 = nextMd5
    return
  }
  if (vaultMd5 === nextMd5) return
  vaultMd5 = nextMd5
  scheduleAutoSync()
}

function scheduleAutoSync() {
  if (!syncStatus.value?.configured || !syncStatus.value.autoSync) return
  if (autoSyncTimer) clearTimeout(autoSyncTimer)
  autoSyncDueAt.value = Date.now() + 60_000
  autoSyncNow.value = Date.now()
  autoSyncState.value = 'pending'
  autoSyncTimer = setTimeout(() => {
    autoSyncTimer = null
    void syncNow(true)
  }, 60_000)
}

function handleSyncConfigurationChanged() {
  void loadSyncStatus().then(() => {
    if (!syncStatus.value?.autoSync && autoSyncTimer) {
      clearTimeout(autoSyncTimer)
      autoSyncTimer = null
      autoSyncDueAt.value = null
      autoSyncState.value = 'idle'
    }
  })
}

const autoSyncBadgeType = computed<'info' | 'success' | 'warning' | 'error' | undefined>(() => {
  switch (autoSyncState.value) {
    case 'pending':
    case 'syncing':
      return 'info'
    case 'success':
      return 'success'
    case 'conflict':
      return 'warning'
    case 'error':
      return 'error'
    default:
      return undefined
  }
})

const autoSyncBadgeTitle = computed(() => {
  if (autoSyncState.value === 'pending' && autoSyncDueAt.value) {
    const seconds = Math.max(0, Math.ceil((autoSyncDueAt.value - autoSyncNow.value) / 1_000))
    return t('sync.autoInSeconds', { seconds })
  }
  return {
    idle: t('sync.title'),
    pending: t('sync.pending'),
    syncing: t('sync.syncing'),
    success: t('sync.success'),
    conflict: t('sync.conflictNotice'),
    error: t('sync.errorNotice'),
  }[autoSyncState.value]
})

function toggleTheme() {
  isDarkTheme.value = !isDarkTheme.value
  localStorage.setItem('my-ssh-theme', isDarkTheme.value ? 'dark' : 'light')
}

function changeLanguage(nextLanguage: string | number) {
  if (nextLanguage === 'zh-CN' || nextLanguage === 'en-US') {
    setLanguage(nextLanguage as AppLanguage)
  }
}

async function minimizeWindow() {
  try { await appWindow.minimize() } catch (error) { console.error('Failed to minimize window:', error) }
}
async function toggleMaximizeWindow() {
  try {
    await appWindow.toggleMaximize()
    isMaximized.value = await appWindow.isMaximized()
  } catch (error) { console.error('Failed to maximize window:', error) }
}
async function closeWindow() {
  try { await appWindow.close() } catch (error) { console.error('Failed to close window:', error) }
}
async function startWindowDrag(event: MouseEvent) {
  if (event.button !== 0) return
  try { await appWindow.startDragging() } catch (error) { console.error('Failed to start window drag:', error) }
}

// --- Profile form ---
const showForm = ref(false)
const editingProfile = ref<SshProfileView | null>(null)

const form = ref<CreateProfileRequest>({
  name: '',
  host: '',
  port: 22,
  username: '',
  auth_type: 'password',
  credential: '',
  key_id: undefined,
  group_name: '',
  icon: 'monitor-cog',
  color: '#3b82f6',
})

const authOptions = computed(() => [
  { label: t('auth.password'), value: 'password' },
  { label: t('auth.key'), value: 'key' },
  { label: t('auth.certificate'), value: 'certificate' },
])

const hostIconOptions = computed<Array<{ id: string; label: string; icon: Component }>>(() => [
  { id: 'server', label: t('icon.server'), icon: Server }, { id: 'server-cog', label: t('icon.serverCog'), icon: ServerCog },
  { id: 'monitor-cog', label: t('icon.workstation'), icon: MonitorCog }, { id: 'terminal', label: t('icon.terminal'), icon: TerminalSquare },
  { id: 'cloud', label: t('icon.cloudHost'), icon: Cloud }, { id: 'cloud-cog', label: t('icon.cloudService'), icon: CloudCog },
  { id: 'database', label: t('icon.database'), icon: Database }, { id: 'hard-drive', label: t('icon.storage'), icon: HardDrive },
  { id: 'container', label: t('icon.container'), icon: Container }, { id: 'box', label: t('icon.virtualMachine'), icon: Box },
  { id: 'network', label: t('icon.network'), icon: Network }, { id: 'router', label: t('icon.router'), icon: Router },
  { id: 'ethernet-port', label: t('icon.switch'), icon: EthernetPort }, { id: 'radio-tower', label: t('icon.gateway'), icon: RadioTower },
  { id: 'globe', label: t('icon.website'), icon: Globe2 }, { id: 'code', label: t('icon.development'), icon: Code2 },
  { id: 'file-code', label: t('icon.codeService'), icon: FileCode2 }, { id: 'workflow', label: t('icon.automation'), icon: Workflow },
  { id: 'layers', label: t('icon.cluster'), icon: Layers3 }, { id: 'waypoints', label: t('icon.proxy'), icon: Waypoints },
  { id: 'cpu', label: t('icon.compute'), icon: Cpu }, { id: 'archive', label: t('icon.backup'), icon: Archive },
  { id: 'shield', label: t('icon.security'), icon: ShieldCheck }, { id: 'zap', label: t('icon.edge'), icon: Zap },
])
const hostIconMap = new Map([
  ['server', Server], ['server-cog', ServerCog], ['monitor-cog', MonitorCog], ['terminal', TerminalSquare],
  ['cloud', Cloud], ['cloud-cog', CloudCog], ['database', Database], ['hard-drive', HardDrive],
  ['container', Container], ['box', Box], ['network', Network], ['router', Router],
  ['ethernet-port', EthernetPort], ['radio-tower', RadioTower], ['globe', Globe2], ['code', Code2],
  ['file-code', FileCode2], ['workflow', Workflow], ['layers', Layers3], ['waypoints', Waypoints],
  ['cpu', Cpu], ['archive', Archive], ['shield', ShieldCheck], ['zap', Zap],
])
const hostColorOptions = ['#3b82f6', '#14b8a6', '#22c55e', '#eab308', '#f97316', '#ef4444', '#ec4899', '#a855f7']
const profileKeyOptions = computed(() => vaultStore.sshKeys
  .filter((key) => form.value.auth_type !== 'certificate' || key.key_type === 'certificate')
  .map((key) => ({ label: key.name, value: key.id })))

function hostIcon(icon: string | null) {
  return hostIconMap.get(icon ?? '') ?? MonitorCog
}

const isEditing = ref(false)
const formError = ref<string | null>(null)
const refreshingProfileId = ref<string | null>(null)

const groupedProfiles = computed(() => {
  const groups = new Map<string, SshProfileView[]>()
  for (const p of vaultStore.profiles) {
    const group = p.group_name || t('hosts.defaultGroup')
    if (!groups.has(group)) groups.set(group, [])
    groups.get(group)!.push(p)
  }
  return groups
})

// --- Lifecycle ---
const activeView = ref<'hosts' | 'keys' | 'scripts'>('hosts')

function closeTerminalPopoversOnOutsideClick(event: PointerEvent) {
  const target = event.target as Node
  if (transferPanelRef.value?.contains(target) || transferButtonRef.value?.contains(target) || scriptPanelRef.value?.contains(target) || scriptButtonRef.value?.contains(target)) return
  transferPanelOpen.value = false
  scriptPanelOpen.value = false
}

function showTransferNotice() {
  if (transferNoticeTimer) clearTimeout(transferNoticeTimer)
  transferNoticeVisible.value = true
  transferNoticeTimer = setTimeout(() => { transferNoticeVisible.value = false }, 3_500)
}

function openTransfers() {
  transferPanelOpen.value = !transferPanelOpen.value
  scriptPanelOpen.value = false
  transferNoticeVisible.value = false
  if (transferPanelOpen.value) transferSeenCount.value = transferStore.tasks.length
}

function openScripts() {
  scriptPanelOpen.value = !scriptPanelOpen.value
  transferPanelOpen.value = false
}

async function insertScriptIntoTerminal(command: string) {
  const sessionId = sessionStore.activeTabId
  if (!sessionId) return
  const inserted = await sessionStore.writeData(sessionId, command.replace(/[\r\n]+$/, ''))
  if (inserted) scriptPanelOpen.value = false
}

onMounted(async () => {
  isMaximized.value = await appWindow.isMaximized()
  await Promise.all([vaultStore.init(), transferStore.initialize(), loadSyncStatus()])
  vaultMd5 = await readVaultMd5()
  vaultMd5Timer = setInterval(() => { void checkVaultMd5() }, 10_000)
  window.addEventListener('sync-configuration-changed', handleSyncConfigurationChanged)
  autoSyncCountdownTimer = setInterval(() => { autoSyncNow.value = Date.now() }, 1_000)
  document.addEventListener('pointerdown', closeTerminalPopoversOnOutsideClick)
})

watch(activeView, (view) => {
  if (view === 'keys') void vaultStore.loadKeys()
})

watch(() => form.value.auth_type, (authType) => {
  if (authType === 'password') return
  void vaultStore.loadKeys().then(() => {
    if (authType !== 'certificate' || !form.value.key_id) return
    const selectedKey = vaultStore.sshKeys.find((key) => key.id === form.value.key_id)
    if (selectedKey?.key_type !== 'certificate') form.value.key_id = undefined
  })
})

watch(() => transferStore.tasks.length, (count, previousCount) => {
  if (count > previousCount) showTransferNotice()
})

// --- Lifecycle ---
// --- Profile CRUD ---
function openCreateForm() {
  isEditing.value = false
  editingProfile.value = null
  formError.value = null
  form.value = {
    name: '',
    host: '',
    port: 22,
    username: 'root',
    auth_type: 'password',
    credential: '',
    key_id: undefined,
    group_name: '',
    icon: 'monitor-cog',
    color: '#3b82f6',
  }
  showForm.value = true
}

function openEditForm(profile: SshProfileView) {
  isEditing.value = true
  editingProfile.value = profile
  formError.value = null
  form.value = {
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    auth_type: profile.auth_type,
    credential: '',
    key_id: profile.key_id || undefined,
    group_name: profile.group_name || '',
    icon: profile.icon || 'monitor-cog',
    color: profile.color || '#3b82f6',
  }
  showForm.value = true
}

async function handleFormSubmit() {
  formError.value = null
  if (!form.value.name.trim() || !form.value.host.trim() || !form.value.username.trim()) {
    formError.value = t('form.requiredFields')
    return
  }

  if (form.value.auth_type === 'password') {
    if (!form.value.credential && !isEditing.value) {
      formError.value = t('form.passwordRequired')
      return
    }
  } else if (!form.value.key_id) {
    formError.value = form.value.auth_type === 'certificate'
      ? t('form.certificateKeyRequired')
      : t('form.keyRequired')
    return
  }

  const data: CreateProfileRequest = {
    name: form.value.name,
    host: form.value.host,
    port: form.value.port || 22,
    username: form.value.username,
    auth_type: form.value.auth_type,
    credential: form.value.auth_type === 'password' ? (form.value.credential || undefined) : undefined,
    key_id: form.value.auth_type !== 'password' ? form.value.key_id : undefined,
    group_name: form.value.group_name || undefined,
    icon: form.value.icon || 'server',
    color: form.value.color || '#3b82f6',
  }

  const profile = isEditing.value && editingProfile.value
    ? await vaultStore.updateProfile(editingProfile.value.id, data)
    : await vaultStore.createProfile(data)

  if (!profile) {
    formError.value = vaultStore.error || t('form.saveFailed')
    return
  }

  showForm.value = false
  editingProfile.value = null
}

async function refreshProfileInfo(profile: SshProfileView) {
  if (refreshingProfileId.value) return
  refreshingProfileId.value = profile.id
  try {
    await vaultStore.refreshProfileInfo(profile.id)
  } finally {
    refreshingProfileId.value = null
  }
}

async function handleDeleteProfile(id: string) {
  await vaultStore.deleteProfile(id)
}

// --- Connection ---
const terminalRefs = ref<Record<string, any>>({})
const activeTerminalInfo = ref<{ host: string; port: number; username: string } | null>(null)

// --- Connection dialog ---
type ConnectionDialogStatus = 'connecting' | 'verifying' | 'authenticating' | 'success' | 'error' | 'host-key-confirm' | 'host-key-changed'
type HostKeyInfo = {
  algorithm: string
  fingerprint: string
  expectedAlgorithm?: string
  expectedFingerprint?: string
}

type ConnectionProgress = {
  stage: 'verifying_host_key' | 'authenticating'
  algorithm?: string
  fingerprint?: string
}

type ConnectionState = {
  profile: SshProfileView
  info: { host: string; port: number; username: string; profileName: string }
  status: ConnectionDialogStatus
  error: string
  hostKey: HostKeyInfo | null
  reconnecting: boolean
}

const connectionStates = ref<Record<string, ConnectionState>>({})
const reconnectVersions = ref<Record<string, number>>({})

function setConnectionState(sessionId: string, state: ConnectionState) {
  connectionStates.value = { ...connectionStates.value, [sessionId]: state }
}

function updateConnectionState(sessionId: string, update: Partial<ConnectionState>) {
  const current = connectionStates.value[sessionId]
  if (!current) return
  setConnectionState(sessionId, { ...current, ...update })
}

function removeConnectionState(sessionId: string) {
  if (!(sessionId in connectionStates.value)) return
  const { [sessionId]: _, ...remaining } = connectionStates.value
  connectionStates.value = remaining
}

async function handleConnect(
  profile: SshProfileView,
  reuseSessionId?: string,
  isReconnect = false,
) {
  const existingTab = sessionStore.tabs.find((tab) => tab.profileId === profile.id)
  if (existingTab && !reuseSessionId) {
    sessionStore.setActiveTab(existingTab.sessionId)
    updateTerminalInfo(existingTab.sessionId)
    await nextTick()
    terminalRefs.value[existingTab.sessionId]?.triggerResize()
    return
  }

  const sessionId = reuseSessionId ?? crypto.randomUUID()
  setConnectionState(sessionId, {
    profile,
    info: {
      host: profile.host,
      port: profile.port,
      username: profile.username,
      profileName: profile.name,
    },
    status: 'connecting',
    error: '',
    hostKey: null,
    reconnecting: isReconnect,
  })
  sessionStore.error = null
  activeTerminalInfo.value = { host: profile.host, port: profile.port, username: profile.username }

  let unlistenProgress: UnlistenFn | undefined
  try {
    // Register before connect_ssh starts so every visual step maps to a real
    // SSH handshake event emitted by the backend.
    unlistenProgress = await listen<ConnectionProgress>(`ssh-connection-progress:${sessionId}`, ({ payload }) => {
      if (!connectionStates.value[sessionId]) return
      if (payload.stage === 'verifying_host_key') {
        updateConnectionState(sessionId, {
          status: 'verifying',
          hostKey: payload.algorithm && payload.fingerprint
            ? { algorithm: payload.algorithm, fingerprint: payload.fingerprint }
            : null,
        })
      } else if (payload.stage === 'authenticating') {
        updateConnectionState(sessionId, { status: 'authenticating' })
      }
    })

    const result = await sessionStore.connect(profile.id, profile.name, sessionId)
    // The tab may be closed while the network request is pending. Its late
    // completion must not change the dialog state for a later connection.
    if (!connectionStates.value[sessionId]) return

    if (result.ok) {
      if (isReconnect) {
        reconnectVersions.value = {
          ...reconnectVersions.value,
          [sessionId]: (reconnectVersions.value[sessionId] ?? 0) + 1,
        }
      }
      updateConnectionState(sessionId, { status: 'success', reconnecting: false })
      await nextTick()
      await new Promise(resolve => setTimeout(resolve, 200))
      terminalRefs.value[sessionId]?.triggerResize()
      await new Promise(resolve => setTimeout(resolve, 600))
      if (connectionStates.value[sessionId]?.status === 'success') removeConnectionState(sessionId)
      return
    }

    const connectionError = result.error || '连接失败'
    const hostKeyError = parseHostKeyError(connectionError)
    if (hostKeyError) {
      updateConnectionState(sessionId, {
        status: hostKeyError.expectedFingerprint ? 'host-key-changed' : 'host-key-confirm',
        error: '',
        hostKey: hostKeyError,
      })
      return
    }
    updateConnectionState(sessionId, { status: 'error', error: connectionError })
  } finally {
    unlistenProgress?.()
  }
}

function parseHostKeyError(error: string): HostKeyInfo | null {
  const message = error.replace(/^Error:\s*/, '')
  const [kind, firstAlgorithm, firstFingerprint, actualAlgorithm, actualFingerprint] = message.split('|')
  if (kind === 'HOST_KEY_UNKNOWN' && firstAlgorithm && firstFingerprint) {
    return { algorithm: firstAlgorithm, fingerprint: firstFingerprint }
  }
  if (kind === 'HOST_KEY_CHANGED' && firstAlgorithm && firstFingerprint && actualAlgorithm && actualFingerprint) {
    return {
      expectedAlgorithm: firstAlgorithm,
      expectedFingerprint: firstFingerprint,
      algorithm: actualAlgorithm,
      fingerprint: actualFingerprint,
    }
  }
  return null
}

async function handleTrustHostKey(sessionId: string) {
  const connection = connectionStates.value[sessionId]
  const hostKey = connection?.hostKey
  if (!connection || !hostKey || hostKey.expectedFingerprint) return
  try {
    await invoke('trust_host_key', {
      host: connection.profile.host,
      port: connection.profile.port,
      algorithm: hostKey.algorithm,
      fingerprint: hostKey.fingerprint,
    })
    await handleConnect(connection.profile, sessionId, connection.reconnecting)
  } catch (error) {
    updateConnectionState(sessionId, { status: 'error', error: String(error) })
  }
}

async function handleRetry(sessionId: string) {
  const connection = connectionStates.value[sessionId]
  if (connection) await handleConnect(connection.profile, sessionId, connection.reconnecting)
}

function handleTerminalDisconnected(sessionId: string, reason: string) {
  const tab = sessionStore.tabs.find((item) => item.sessionId === sessionId)
  const profile = tab && vaultStore.profiles.find((item) => item.id === tab.profileId)
  if (!tab || !profile) return

  sessionStore.setActiveTab(sessionId)
  activeTerminalInfo.value = { host: profile.host, port: profile.port, username: profile.username }
  setConnectionState(sessionId, {
    profile,
    info: {
      host: profile.host,
      port: profile.port,
      username: profile.username,
      profileName: profile.name,
    },
    status: 'error',
    error: reason,
    hostKey: null,
    reconnecting: true,
  })
}

function clearConnectionDialogForSession(sessionId: string) {
  removeConnectionState(sessionId)
}

function handleCloseConnDialog(sessionId: string) {
  clearConnectionDialogForSession(sessionId)
  sessionStore.closeTab(sessionId)
}

function updateTerminalInfo(sessionId: string) {
  const tab = sessionStore.tabs.find(t => t.sessionId === sessionId)
  if (tab) {
    const profile = vaultStore.profiles.find(p => p.id === tab.profileId)
    if (profile) {
      activeTerminalInfo.value = { host: profile.host, port: profile.port, username: profile.username }
    }
  }
}

function handleTabClick(sessionId: string) {
  sessionStore.setActiveTab(sessionId)
  updateTerminalInfo(sessionId)
  nextTick(() => {
    terminalRefs.value[sessionId]?.triggerResize()
  })
}

function handleCloseTab(sessionId: string) {
  clearConnectionDialogForSession(sessionId)
  sftpOpenSessions.value.delete(sessionId)
  sftpOpenSessions.value = new Set(sftpOpenSessions.value)
  aiOpenSessions.value.delete(sessionId)
  aiOpenSessions.value = new Set(aiOpenSessions.value)
  sessionStore.closeTab(sessionId)
  if (sessionStore.activeTabId) {
    updateTerminalInfo(sessionStore.activeTabId)
  } else {
    activeTerminalInfo.value = null
  }
}

// --- SFTP ---
// 每个 session 独立跟踪 SFTP 是否打开
const sftpOpenSessions = ref<Set<string>>(new Set())
const aiOpenSessions = ref<Set<string>>(new Set())
const transferPanelOpen = ref(false)
const scriptPanelOpen = ref(false)
const transferSeenCount = ref(0)
const hasUnreadTransfers = computed(() => transferStore.tasks.length > transferSeenCount.value)
const sftpPanelWidth = ref(400)
const aiPanelWidth = ref(480)
const permissionTarget = ref<{ name: string; path: string; mode: number; sessionId: string } | null>(null)
type ActionDialogRequest = {
  kind: 'input' | 'confirm'
  title: string
  message?: string
  initialValue?: string
  placeholder?: string
  confirmText?: string
  danger?: boolean
  onConfirm: (value: string) => void | Promise<void>
}
const actionDialogRequest = ref<ActionDialogRequest | null>(null)
let panelResizeStartX = 0
let panelResizeStartWidth = 0
let panelBeingResized: 'sftp' | 'ai' | null = null

// 当前页签是否打开了 SFTP
const currentSftpOpen = computed(() => {
  return !!sessionStore.activeTabId && sftpOpenSessions.value.has(sessionStore.activeTabId)
})
const currentAiOpen = computed(() => {
  return !!sessionStore.activeTabId && aiOpenSessions.value.has(sessionStore.activeTabId)
})

function openSftp() {
  const sid = sessionStore.activeTabId
  if (!sid || !activeTerminalInfo.value) return
  if (sftpOpenSessions.value.has(sid)) {
    sftpOpenSessions.value.delete(sid)
  } else {
    sftpOpenSessions.value.add(sid)
    aiOpenSessions.value.delete(sid)
    aiOpenSessions.value = new Set(aiOpenSessions.value)
  }
  sftpOpenSessions.value = new Set(sftpOpenSessions.value)
}



function closeSftp() {
  const sid = sessionStore.activeTabId
  if (!sid) return
  sftpOpenSessions.value.delete(sid)
  sftpOpenSessions.value = new Set(sftpOpenSessions.value)
}

function openAiChat() {
  const sid = sessionStore.activeTabId
  if (!sid) return
  if (aiOpenSessions.value.has(sid)) {
    aiOpenSessions.value.delete(sid)
  } else {
    aiOpenSessions.value.add(sid)
    sftpOpenSessions.value.delete(sid)
    sftpOpenSessions.value = new Set(sftpOpenSessions.value)
  }
  aiOpenSessions.value = new Set(aiOpenSessions.value)
}

function closeAiChat() {
  const sid = sessionStore.activeTabId
  if (!sid) return
  aiOpenSessions.value.delete(sid)
  aiOpenSessions.value = new Set(aiOpenSessions.value)
}

function startPanelResize(panel: 'sftp' | 'ai', event: MouseEvent) {
  panelBeingResized = panel
  panelResizeStartX = event.clientX
  panelResizeStartWidth = panel === 'ai' ? aiPanelWidth.value : sftpPanelWidth.value
  window.addEventListener('mousemove', resizePanel)
  window.addEventListener('mouseup', stopPanelResize)
}

function resizePanel(event: MouseEvent) {
  if (!panelBeingResized) return
  const nextWidth = Math.min(720, Math.max(320, panelResizeStartWidth + panelResizeStartX - event.clientX))
  if (panelBeingResized === 'ai') aiPanelWidth.value = nextWidth
  else sftpPanelWidth.value = nextWidth
}

function stopPanelResize() {
  panelBeingResized = null
  window.removeEventListener('mousemove', resizePanel)
  window.removeEventListener('mouseup', stopPanelResize)
}

function openPermissions(file: { name: string; mode: number }, path: string) {
  if (!sessionStore.activeTabId) return
  permissionTarget.value = { name: file.name, path, mode: file.mode, sessionId: sessionStore.activeTabId }
}

function requestSftpInput(options: { title: string; initialValue?: string; placeholder?: string; onConfirm: (value: string) => void }) {
  actionDialogRequest.value = { kind: 'input', confirmText: '确认', ...options }
}

function requestSftpConfirmation(options: { title: string; message: string; confirmText: string; danger?: boolean; onConfirm: () => void }) {
  actionDialogRequest.value = { kind: 'confirm', ...options, onConfirm: () => options.onConfirm() }
}

async function confirmActionDialog(value: string) {
  const request = actionDialogRequest.value
  if (!request) return
  actionDialogRequest.value = null
  await request.onConfirm(value)
}

async function applyPermissions(mode: number) {
  const target = permissionTarget.value
  if (!target) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('sftp_set_permissions', { sessionId: target.sessionId, path: target.path, mode })
    permissionTarget.value = null
  } catch (error) {
    console.error('Failed to update permissions:', error)
  }
}

// --- Server Stats ---
const serverStats = ref<{ cpu: string; memory: string; disk: string; netUp: string; netDown: string; latency: string } | null>(null)
let statsTimer: ReturnType<typeof setTimeout> | null = null
let statsRequestInFlight = false
let statsGeneration = 0

async function fetchServerStats(sessionId: string, generation: number) {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const stats = await invoke<{
      cpu: string
      memory: string
      disk: string
      net_up: string
      net_down: string
      latency: string
    }>('get_server_stats', { sessionId })
    if (sessionStore.activeTabId !== sessionId || generation !== statsGeneration) return
    serverStats.value = {
      cpu: stats.cpu,
      memory: stats.memory,
      disk: stats.disk,
      netUp: stats.net_up,
      netDown: stats.net_down,
      latency: stats.latency,
    }
  } catch (e) {
    // 忽略错误
  }
}

async function scheduleServerStats(generation: number) {
  const sessionId = sessionStore.activeTabId
  if (!sessionId || generation !== statsGeneration || document.hidden) return
  if (!statsRequestInFlight) {
    statsRequestInFlight = true
    await fetchServerStats(sessionId, generation)
    statsRequestInFlight = false
  }
  if (sessionStore.activeTabId && generation === statsGeneration && !document.hidden) {
    statsTimer = setTimeout(() => { void scheduleServerStats(generation) }, 10_000)
  }
}

function stopServerStats() {
  statsGeneration += 1
  if (statsTimer) {
    clearTimeout(statsTimer)
    statsTimer = null
  }
}

function restartServerStats() {
  stopServerStats()
  serverStats.value = null
  if (!sessionStore.activeTabId || document.hidden) return
  const generation = statsGeneration
  void scheduleServerStats(generation)
}

function handleVisibilityChange() {
  restartServerStats()
}

watch(() => sessionStore.activeTabId, restartServerStats)
onMounted(() => document.addEventListener('visibilitychange', handleVisibilityChange))
onBeforeUnmount(() => {
  stopServerStats()
  transferStore.dispose()
  if (transferNoticeTimer) clearTimeout(transferNoticeTimer)
  if (autoSyncTimer) clearTimeout(autoSyncTimer)
  if (autoSyncCountdownTimer) clearInterval(autoSyncCountdownTimer)
  if (vaultMd5Timer) clearInterval(vaultMd5Timer)
  window.removeEventListener('sync-configuration-changed', handleSyncConfigurationChanged)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  document.removeEventListener('pointerdown', closeTerminalPopoversOnOutsideClick)
  window.removeEventListener('error', handleWindowError)
  window.removeEventListener('unhandledrejection', handleUnhandledRejection)
})

// --- Settings ---
const showSettings = ref(false)
const settingsSection = ref<'terminal' | 'ai' | 'sync' | 'system'>('terminal')
const showDiagnosticExportConfirm = ref(false)
const diagnosticExporting = ref(false)

async function confirmDiagnosticExport() {
  diagnosticExporting.value = true
  try {
    const path = await invoke<string>('export_diagnostic_bundle')
    showDiagnosticExportConfirm.value = false
    window.alert(t('diagnostics.success', { path }))
  } catch {
    window.alert(t('diagnostics.failed'))
  } finally {
    diagnosticExporting.value = false
  }
}

function recordFrontendCrash(kind: 'error' | 'unhandled_rejection', message: string, stack?: string) {
  void invoke('record_frontend_crash', { kind, message, stack }).catch(() => {})
}

function handleWindowError(event: ErrorEvent) {
  recordFrontendCrash('error', event.message || 'Unhandled frontend error', event.error?.stack)
}

function handleUnhandledRejection(event: PromiseRejectionEvent) {
  const message = event.reason instanceof Error ? event.reason.message : String(event.reason)
  const stack = event.reason instanceof Error ? event.reason.stack : undefined
  recordFrontendCrash('unhandled_rejection', message, stack)
}

window.addEventListener('error', handleWindowError)
window.addEventListener('unhandledrejection', handleUnhandledRejection)

function openSettings() {
  settingsSection.value = 'terminal'
  showSettings.value = true
}

function openAiSettings() {
  settingsSection.value = 'ai'
  showSettings.value = true
}

function openSyncSettings() {
  settingsSection.value = 'sync'
  showSettings.value = true
}

</script>

<template>
  <n-config-provider :theme="naiveTheme" :theme-overrides="naiveThemeOverrides" :locale="naiveLocale" :date-locale="naiveDateLocale">
    <n-global-style />
    <n-message-provider>
      <!-- 主应用 -->
      <div class="app-layout" :class="{ 'theme-dark': isDarkTheme }">
        <!-- Tab bar - 始终显示 -->
        <header class="tab-bar">
          <div class="tabs-container">
            <div
              class="tab home-tab"
              :class="{ active: !sessionStore.activeTabId }"
              @click="sessionStore.activeTabId = null; activeTerminalInfo = null"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2 2z"/>
                <polyline points="9 22 9 12 15 12 15 22"/>
              </svg>
            </div>

            <div
              v-for="tab in sessionStore.tabs"
              :key="tab.sessionId"
              class="tab"
              :class="{ active: sessionStore.activeTabId === tab.sessionId }"
              @click="handleTabClick(tab.sessionId)"
            >
              <span class="tab-title">{{ tab.profileName }}</span>
              <span class="tab-dot" :title="t('app.connected')" />
              <span class="tab-close" @click.stop="handleCloseTab(tab.sessionId)">×</span>
            </div>

            <div class="tab new-tab" :title="t('app.hostList')" :aria-label="t('app.hostList')" @click="sessionStore.activeTabId = null; activeTerminalInfo = null">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <path d="M12 5v14M5 12h14" />
              </svg>
            </div>
          </div>
          <div class="titlebar-drag-region" data-tauri-drag-region @mousedown="startWindowDrag" />
          <div class="titlebar-actions" :aria-label="t('app.features')">
            <n-dropdown trigger="click" :options="languageOptions" :value="language" @select="changeLanguage">
              <button class="titlebar-language" :title="t('app.language', { language: languageLabel })" :aria-label="t('app.switchLanguage')">
                <Languages :size="16" />
                <span>{{ languageLabel }}</span>
              </button>
            </n-dropdown>
            <n-popover trigger="click" placement="bottom-end" :show="syncPopoverVisible" @update:show="handleSyncPopoverShow">
              <template #trigger>
                <button class="titlebar-action" :class="{ active: syncPopoverVisible }" :title="autoSyncBadgeTitle" :aria-label="t('sync.title')">
                  <span class="sync-cloud-icon">
                    <Cloud :size="17" />
                    <span v-if="autoSyncBadgeType" class="sync-status-dot" :class="`is-${autoSyncBadgeType}`" />
                  </span>
                </button>
              </template>
              <section class="quick-sync-panel">
                <header>
                  <strong>{{ t('sync.title') }}</strong>
                  <button class="quick-sync-settings" :class="{ enabled: syncStatus?.configured }" :title="t('sync.openSettings')" @click="openSyncSettings(); syncPopoverVisible = false">
                    <span :class="{ enabled: syncStatus?.configured }">{{ syncStatus?.configured ? (syncStatus.autoSync ? t('sync.auto') : t('sync.manual')) : t('sync.configure') }}</span>
                    <Settings :size="15" />
                  </button>
                </header>
                <template v-if="syncStatus?.configured">
                  <div class="quick-sync-provider"><Cloud :size="18" /><span>{{ syncProviderLabel }}</span></div>
                  <dl>
                    <div><dt>{{ t('sync.file') }}</dt><dd>{{ syncStatus.remoteFileName }}</dd></div>
                    <div><dt>{{ t('sync.localVersion') }}</dt><dd>v{{ remoteSyncStatus?.localVaultRevision ?? syncStatus.localVaultRevision }}</dd></div>
                    <div><dt>{{ t('sync.remoteVersion') }}</dt><dd>{{ remoteSyncStatus ? `v${remoteSyncStatus.remoteVaultRevision}` : t('sync.notChecked') }}</dd></div>
                    <div><dt>{{ t('sync.currentState') }}</dt><dd>{{ syncVersionState }}</dd></div>
                    <div v-if="remoteSyncStatus?.remoteUpdatedAt"><dt>{{ t('sync.remoteUpdatedAt') }}</dt><dd>{{ formatDate(remoteSyncStatus.remoteUpdatedAt) }}</dd></div>
                    <div><dt>{{ t('sync.lastSyncedAt') }}</dt><dd>{{ syncStatus.lastSyncedAt ? formatDate(syncStatus.lastSyncedAt) : t('sync.notSynced') }}</dd></div>
                  </dl>
                  <n-alert v-if="remoteSyncAction" type="info" :show-icon="false">{{ remoteSyncAction }}</n-alert>
                  <n-alert v-if="syncError" type="error" :show-icon="false">{{ syncError }}</n-alert>
                  <n-alert v-if="syncNotice" type="success" :show-icon="false">{{ syncNotice }}</n-alert>
                  <div class="quick-sync-actions">
                    <n-button secondary :loading="remoteSyncLoading" @click="checkRemoteSyncStatus">{{ t('sync.check') }}</n-button>
                    <n-button type="primary" :loading="syncLoading" @click="syncNow()">{{ t('sync.now') }}</n-button>
                  </div>
                </template>
                <template v-else>
                  <p>{{ t('sync.connectDescription') }}</p>
                  <n-alert v-if="syncError" type="error" :show-icon="false">{{ syncError }}</n-alert>
                </template>
              </section>
            </n-popover>
            <button class="titlebar-action" :title="isDarkTheme ? t('app.lightTheme') : t('app.darkTheme')" :aria-label="isDarkTheme ? t('app.lightTheme') : t('app.darkTheme')" @click="toggleTheme"><Sun v-if="isDarkTheme" :size="17" /><Moon v-else :size="17" /></button>
          </div>
          <div class="window-controls">
            <button class="window-control" :title="t('app.minimize')" :aria-label="t('app.minimize')" @click="minimizeWindow"><svg class="minimize-icon" width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M2.5 7h9" /></svg></button>
            <button class="window-control" :title="isMaximized ? t('app.restore') : t('app.maximize')" :aria-label="isMaximized ? t('app.restore') : t('app.maximize')" @click="toggleMaximizeWindow"><Copy v-if="isMaximized" :size="14" /><Square v-else :size="13" /></button>
            <button class="window-control close" :title="t('app.close')" :aria-label="t('app.close')" @click="closeWindow"><X :size="17" /></button>
          </div>
        </header>

        <!-- 紧凑连接栏 -->
        <div v-if="sessionStore.activeTabId && activeTerminalInfo" class="terminal-toolbar">
          <div class="toolbar-left">
            <span class="toolbar-info">{{ activeTerminalInfo.username }}@{{ activeTerminalInfo.host }}:{{ activeTerminalInfo.port }}</span>
            <span v-if="serverStats" class="toolbar-stats">
              <span class="stat-item"><Cpu :size="11" />{{ serverStats.cpu }}</span>
              <span class="stat-item"><MemoryStick :size="11" />{{ serverStats.memory }}</span>
              <span class="stat-item"><HardDrive :size="11" />{{ serverStats.disk }}</span>
              <span class="stat-item"><Download :size="11" />{{ serverStats.netDown }}</span>
              <span class="stat-item"><Upload :size="11" />{{ serverStats.netUp }}</span>
              <span class="stat-item"><Zap :size="11" />{{ serverStats.latency }}</span>
            </span>
          </div>
          <div class="toolbar-right">
            <button class="toolbar-btn" :class="{ active: currentAiOpen }" title="AI 对话" @click="openAiChat"><Sparkles :size="16" /></button>
            <button ref="scriptButtonRef" class="toolbar-btn" :class="{ active: scriptPanelOpen }" title="脚本" aria-label="脚本" @click="openScripts"><FileCode2 :size="16" /></button>
            <div class="transfer-control">
              <button ref="transferButtonRef" class="toolbar-btn transfer-button" :class="{ active: transferPanelOpen, unread: hasUnreadTransfers }" title="传输任务" @click="openTransfers"><Download :size="16" /><span v-if="hasUnreadTransfers" class="transfer-badge" /></button>
              <div v-if="transferNoticeVisible && transferStore.tasks[0]" class="transfer-notice" role="status" @pointerdown.stop>
                <strong>任务已添加</strong>
                <span>{{ transferStore.tasks[0].direction === 'upload' ? '上传' : '下载' }}：{{ transferStore.tasks[0].name }}</span>
                <button @click="openTransfers">查看传输</button>
              </div>
            </div>
            <button class="toolbar-btn" :class="{ active: currentSftpOpen }" title="SFTP 文件管理" @click="openSftp">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
            </button>
          </div>
        </div>

        <div
          v-if="scriptPanelOpen && sessionStore.activeTabId"
          ref="scriptPanelRef"
          class="main-script-panel"
        >
          <ScriptPanel @insert="insertScriptIntoTerminal" />
        </div>
        <div
          v-if="transferPanelOpen && sessionStore.activeTabId"
          ref="transferPanelRef"
          class="main-transfer-panel"
        >
          <TransferPanel :session-id="sessionStore.activeTabId" standalone />
        </div>

        <!-- 内容区域 -->
        <div class="content-area">
          <!-- 终端内容 - v-show 保持存活 -->
          <div
            v-for="tab in sessionStore.tabs"
            :key="tab.sessionId"
            class="terminal-tab"
            v-show="sessionStore.activeTabId === tab.sessionId"
          >
            <Terminal
              :ref="(el: any) => { if (el) terminalRefs[tab.sessionId] = el }"
              :session-id="tab.sessionId"
              :dark="isDarkTheme"
              :reconnect-version="reconnectVersions[tab.sessionId]"
              @disconnected="handleTerminalDisconnected(tab.sessionId, $event)"
            />
            <ConnectionDialog
              v-if="connectionStates[tab.sessionId]"
              :show="true"
              :host="connectionStates[tab.sessionId].info.host"
              :port="connectionStates[tab.sessionId].info.port"
              :username="connectionStates[tab.sessionId].info.username"
              :profile-name="connectionStates[tab.sessionId].info.profileName"
              :icon="hostIcon(connectionStates[tab.sessionId].profile.icon ?? null)"
              :color="connectionStates[tab.sessionId].profile.color || '#3b82f6'"
              :status="connectionStates[tab.sessionId].status"
              :error="connectionStates[tab.sessionId].error"
              :host-key="connectionStates[tab.sessionId].hostKey ?? undefined"
              :dark="isDarkTheme"
              @trust-host-key="handleTrustHostKey(tab.sessionId)"
              @retry="handleRetry(tab.sessionId)"
              @close="handleCloseConnDialog(tab.sessionId)"
            />
          </div>

          <!-- 首页内容 - 无页签激活时显示 -->
          <div v-if="!sessionStore.activeTabId" class="home-content">
            <div class="home-layout">
              <!-- Sidebar -->
              <div class="sidebar">
                <div class="sidebar-logo">
                  <img class="logo-icon" src="/logo.png" alt="MJJSSH" />
                                    <span class="logo-text">MJJSSH</span>
                </div>

                <div class="sidebar-nav">
                  <div class="nav-item" :class="{ active: activeView === 'hosts' }" @click="activeView = 'hosts'">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
                      <line x1="8" y1="21" x2="16" y2="21"/>
                      <line x1="12" y1="17" x2="12" y2="21"/>
                    </svg>
                    <span>{{ t('nav.hosts') }}</span>
                  </div>
                  <div class="nav-item" :class="{ active: activeView === 'keys' }" @click="activeView = 'keys'">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>
                    </svg>
                    <span>{{ t('nav.keys') }}</span>
                  </div>
                  <div class="nav-item" :class="{ active: activeView === 'scripts' }" @click="activeView = 'scripts'">
                    <FileCode2 :size="16" />
                    <span>{{ t('nav.scripts') }}</span>
                  </div>
                </div>

                <div class="sidebar-bottom">
                  <div class="nav-item" @click="openSettings">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <circle cx="12" cy="12" r="3"/>
                      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                    <span>{{ t('nav.settings') }}</span>
                  </div>
                </div>
              </div>

              <!-- Main content -->
              <div class="main-content">
                <!-- Keys view -->
                <KeysView v-if="activeView === 'keys'" />
                <ScriptsView v-else-if="activeView === 'scripts'" />

                <!-- Hosts view -->
                <template v-else>
                <div class="content-header">
                  <div class="header-left">
                    <h2>{{ t('nav.hosts') }}</h2>
                    <span class="host-count">{{ t('hosts.count', { count: vaultStore.profiles.length }) }}</span>
                  </div>
                  <n-button type="primary" @click="openCreateForm">
                    <template #icon>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <line x1="12" y1="5" x2="12" y2="19"/>
                        <line x1="5" y1="12" x2="19" y2="12"/>
                      </svg>
                    </template>
                    {{ t('hosts.new') }}
                  </n-button>
                </div>



                <n-empty v-if="vaultStore.profiles.length === 0 && !vaultStore.loading" :description="t('hosts.empty')" style="padding: 60px 0">
                  <template #extra>
                    <n-button type="primary" @click="openCreateForm">{{ t('hosts.createFirst') }}</n-button>
                  </template>
                </n-empty>

                <div v-else v-for="[group, items] in groupedProfiles" :key="group" class="host-group">
                  <div class="group-header">
                    <span class="group-name">{{ group }}</span>
                    <span class="group-count">{{ t('hosts.count', { count: items.length }) }}</span>
                  </div>

                  <div class="host-grid">
                    <EntityCard
                      v-for="profile in items"
                      :key="profile.id"
                      class="host-card"
                      :icon="hostIcon(profile.icon)"
                      :color="profile.color || '#3b82f6'"
                      :title="profile.name"
                      :subtitle="`${profile.username}@${profile.host}`"
                      @dblclick="handleConnect(profile)"
                    >
                      <template #actions>
                        <div class="host-actions" @click.stop>
                          <n-button size="tiny" quaternary :title="t('hosts.edit')" :aria-label="t('hosts.edit')" @click="openEditForm(profile)">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                          </n-button>
                          <n-popconfirm @positive-click="handleDeleteProfile(profile.id)">
                            <template #trigger>
                              <n-button size="tiny" quaternary type="error" :title="t('hosts.delete')" :aria-label="t('hosts.delete')"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2 2V6m3 0V4a2 2 0 0 1 2 2v2"/></svg></n-button>
                            </template>
                            {{ t('hosts.deleteConfirm', { name: profile.name }) }}
                          </n-popconfirm>
                        </div>
                      </template>
                      <template #footer>
                        <div v-if="profile.os || profile.location" class="host-meta-row">
                          <div v-if="profile.os" class="host-meta" :title="profile.os">{{ profile.os }}</div>
                          <div v-if="profile.location" class="host-meta location" :title="profile.location"><MapPin :size="14" />{{ profile.location }}</div>
                        </div>
                        <button
                          v-else
                          type="button"
                          class="host-info-refresh"
                          :disabled="Boolean(refreshingProfileId)"
                          @click.stop="refreshProfileInfo(profile)"
                        >
                          <RefreshCw :size="14" :class="{ 'is-spinning': refreshingProfileId === profile.id }" />
                          {{ t('hosts.refreshInfo') }}
                        </button>
                      </template>
                    </EntityCard>
                  </div>
                </div>
                </template>
              </div>
            </div>
          </div>

          <!-- SFTP 右侧面板，与终端共享内容区域高度 -->
          <div
            v-show="currentSftpOpen"
            class="sftp-panel"
            :style="{ width: `${sftpPanelWidth}px` }"
          >
            <div class="sftp-resize-handle" @mousedown.prevent="startPanelResize('sftp', $event)" />
            <div class="sftp-panel-body">
              <template v-for="tab in sessionStore.tabs" :key="tab.sessionId">
                <SftpView
                  v-if="sftpOpenSessions.has(tab.sessionId)"
                  v-show="sessionStore.activeTabId === tab.sessionId"
                  :session-id="tab.sessionId"
                  :dark="isDarkTheme"
                  @close="closeSftp"
                  @edit-permissions="openPermissions"
                  @request-input="requestSftpInput"
                  @request-confirm="requestSftpConfirmation"
                />
              </template>
            </div>
          </div>

          <div
            v-show="currentAiOpen"
            class="sftp-panel ai-panel"
            :style="{ width: `${aiPanelWidth}px` }"
          >
            <div class="sftp-resize-handle" @mousedown.prevent="startPanelResize('ai', $event)" />
            <div class="sftp-panel-body">
              <template v-for="tab in sessionStore.tabs" :key="tab.sessionId">
                <AiChatPanel
                  v-if="aiOpenSessions.has(tab.sessionId)"
                  v-show="sessionStore.activeTabId === tab.sessionId"
                  :session-id="tab.sessionId"
                  @close="closeAiChat"
                  @open-ai-settings="openAiSettings"
                />
              </template>
            </div>
          </div>
        </div>

        <!-- Profile form modal -->
        <n-modal
          v-model:show="showForm"
          :title="isEditing ? t('form.editHost') : t('form.createHost')"
          preset="card"
          style="width: 500px"
        >
          <n-form :model="form" label-placement="left" label-width="80">
            <n-alert v-if="formError" type="error" closable style="margin-bottom: 16px" @close="formError = null">
              {{ formError }}
            </n-alert>
            <n-form-item :label="t('form.name')" required>
              <n-input v-model:value="form.name" placeholder="My Server" />
            </n-form-item>
            <n-form-item :label="t('form.host')" required>
              <n-input v-model:value="form.host" placeholder="192.168.1.100" />
            </n-form-item>
            <n-form-item :label="t('form.port')">
              <n-input-number v-model:value="form.port" :min="1" :max="65535" style="width: 120px" />
            </n-form-item>
            <n-form-item :label="t('form.username')" required>
              <n-input v-model:value="form.username" placeholder="root" />
            </n-form-item>
            <n-form-item :label="t('form.auth')">
              <n-select v-model:value="form.auth_type" :options="authOptions" />
            </n-form-item>
            <n-form-item v-if="form.auth_type === 'password'" :label="t('form.password')" required>
              <n-input
                v-model:value="form.credential"
                type="password"
                :placeholder="t('form.passwordPlaceholder')"
                show-password-on="click"
              />
            </n-form-item>
            <n-form-item v-if="form.auth_type !== 'password'" :label="t('form.key')" required>
              <n-select
                :value="form.key_id"
                :options="profileKeyOptions"
                :placeholder="t('form.keyPlaceholder')"
                @update:value="(val: string) => { form.key_id = val }"
              />
              <n-button v-if="vaultStore.sshKeys.length === 0" size="small" type="primary" style="margin-left: 8px" @click="activeView = 'keys'; showForm = false">
                {{ t('form.createKey') }}
              </n-button>
            </n-form-item>
            <n-form-item :label="t('form.group')">
              <n-input v-model:value="form.group_name" :placeholder="t('form.groupPlaceholder')" />
            </n-form-item>
            <n-form-item :label="t('form.icon')">
              <div class="host-icon-picker" role="listbox" :aria-label="t('form.hostIcon')">
                <button v-for="option in hostIconOptions" :key="option.id" type="button" :class="{ selected: form.icon === option.id }" :title="option.label" :aria-label="option.label" @click="form.icon = option.id"><component :is="option.icon" :size="17" /></button>
              </div>
            </n-form-item>
            <n-form-item :label="t('form.color')">
              <div class="host-color-picker" role="radiogroup" :aria-label="t('form.hostColor')">
                <button v-for="color in hostColorOptions" :key="color" type="button" :class="{ selected: form.color === color }" :style="{ '--picker-color': color }" :title="color" :aria-label="t('form.selectColor', { color })" @click="form.color = color" />
              </div>
            </n-form-item>
          </n-form>

          <template #footer>
            <n-space justify="end">
              <n-button
                v-if="isEditing && editingProfile"
                :loading="refreshingProfileId === editingProfile.id"
                :disabled="Boolean(refreshingProfileId)"
                @click="refreshProfileInfo(editingProfile)"
              >
                {{ t('hosts.refreshInfo') }}
              </n-button>
              <n-button @click="showForm = false; formError = null">{{ t('form.cancel') }}</n-button>
              <n-button type="primary" :loading="vaultStore.loading" @click="handleFormSubmit">
                {{ isEditing ? t('form.save') : t('form.create') }}
              </n-button>
            </n-space>
          </template>
        </n-modal>


        <div v-if="showSettings" class="settings-overlay" role="dialog" aria-modal="true" :aria-label="t('settings.title')">
          <section class="settings-window">
            <header class="settings-titlebar">
              <h2>{{ t('settings.title') }}</h2>
              <button class="settings-close" :aria-label="t('settings.close')" @click="showSettings = false"><X :size="18" /></button>
            </header>
            <div class="settings-body">
              <nav class="settings-nav" :aria-label="t('settings.categories')">
                <button :class="{ active: settingsSection === 'terminal' }" @click="settingsSection = 'terminal'">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="4" width="18" height="15" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/></svg>
                  {{ t('settings.terminal') }}
                </button>
                <button :class="{ active: settingsSection === 'ai' }" @click="settingsSection = 'ai'"><Sparkles :size="16" />AI</button>
                <button :class="{ active: settingsSection === 'sync' }" @click="settingsSection = 'sync'"><Cloud :size="16" />{{ t('sync.title') }}</button>
                <button :class="{ active: settingsSection === 'system' }" @click="settingsSection = 'system'">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="3" width="18" height="18" rx="3"/><path d="M8 8h8M8 12h8M8 16h5"/></svg>
                  {{ t('settings.system') }}
                </button>
              </nav>
              <main class="settings-content">
                <template v-if="settingsSection === 'terminal'">
                  <h3>{{ t('settings.terminal') }}</h3>
                  <div class="settings-panel">
                    <div class="settings-row"><div><strong>{{ t('settings.renderer') }}</strong><p>{{ t('settings.rendererDescription') }}</p></div><span class="settings-value">{{ t('settings.enabled') }}</span></div>
                    <div class="settings-row"><div><strong>{{ t('settings.buffer') }}</strong><p>{{ t('settings.bufferDescription') }}</p></div><span class="settings-value">{{ t('settings.lines') }}</span></div>
                  </div>
                </template>
                <AiSettings v-else-if="settingsSection === 'ai'" />
                <SyncSettings v-else-if="settingsSection === 'sync'" />
                <template v-else>
                  <h3>{{ t('settings.system') }}</h3>
                  <div class="settings-panel">
                    <div class="settings-row"><div><strong>MJJSSH</strong><p>{{ t('settings.appDescription') }}</p></div><span class="settings-value">v0.1.0</span></div>
                    <div class="settings-row"><div><strong>{{ t('settings.theme') }}</strong><p>{{ t('settings.themeDescription', { theme: isDarkTheme ? t('settings.dark') : t('settings.light') }) }}</p></div><span class="settings-value">{{ isDarkTheme ? t('settings.dark') : t('settings.light') }}</span></div>
                  </div>
                  <h3>{{ t('diagnostics.title') }}</h3>
                  <div class="settings-panel">
                    <div class="settings-row"><div><strong>{{ t('diagnostics.title') }}</strong><p>{{ t('diagnostics.description') }}</p></div><n-button size="small" @click="showDiagnosticExportConfirm = true">{{ t('diagnostics.export') }}</n-button></div>
                  </div>
                </template>
              </main>
            </div>
          </section>
        </div>

        <n-modal v-model:show="showDiagnosticExportConfirm" preset="dialog" :title="t('diagnostics.confirmTitle')" :mask-closable="!diagnosticExporting" :closable="!diagnosticExporting">
          <div class="diagnostic-export-confirmation">
            <strong>{{ t('diagnostics.includedTitle') }}</strong>
            <p>{{ t('diagnostics.includedItems') }}</p>
            <strong>{{ t('diagnostics.excludedTitle') }}</strong>
            <p>{{ t('diagnostics.excludedItems') }}</p>
            <n-alert type="warning" :show-icon="false">{{ t('diagnostics.warning') }}</n-alert>
          </div>
          <template #action>
            <n-space justify="end">
              <n-button :disabled="diagnosticExporting" @click="showDiagnosticExportConfirm = false">{{ t('diagnostics.cancel') }}</n-button>
              <n-button type="primary" :loading="diagnosticExporting" @click="confirmDiagnosticExport">{{ diagnosticExporting ? t('diagnostics.exporting') : t('diagnostics.confirm') }}</n-button>
            </n-space>
          </template>
        </n-modal>

        <PermissionsDialog
          :show="Boolean(permissionTarget)"
          :name="permissionTarget?.name ?? ''"
          :mode="permissionTarget?.mode ?? 0"
          @close="permissionTarget = null"
          @apply="applyPermissions"
        />
        <ActionDialog
          :show="Boolean(actionDialogRequest)"
          :kind="actionDialogRequest?.kind ?? 'confirm'"
          :title="actionDialogRequest?.title ?? ''"
          :message="actionDialogRequest?.message"
          :initial-value="actionDialogRequest?.initialValue"
          :placeholder="actionDialogRequest?.placeholder"
          :confirm-text="actionDialogRequest?.confirmText"
          :danger="actionDialogRequest?.danger"
          @close="actionDialogRequest = null"
          @confirm="confirmActionDialog"
        />
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
.app-layout {
  --app-base: #f8fafc;
  --app-surface: #ffffff;
  --app-elevated: #f1f5f9;
  --app-border: #dbe3ef;
  --app-text: #1e293b;
  --app-muted: #64748b;
  --app-hover: #e8eef7;
  --app-accent: #2563eb;
  --app-panel: #ffffff;
  --app-code: #f1f5f9;
  --app-selection: #dbeafe;
  --app-shadow: rgba(15, 23, 42, .16);
  --app-terminal: #f8fafc;
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

.app-layout.theme-dark {
  --app-base: #1e1e2e;
  --app-surface: #181825;
  --app-elevated: #1c2330;
  --app-border: #344057;
  --app-text: #cdd6f4;
  --app-muted: #9aa8be;
  --app-hover: #252e3e;
  --app-accent: #89b4fa;
  --app-panel: #151a25;
  --app-code: #111722;
  --app-selection: #30394b;
  --app-shadow: rgba(0, 0, 0, .32);
  --app-terminal: #111722;
}

/* ==================== Tab Bar ==================== */
.tab-bar {
  display: flex;
  align-items: center;
  height: 40px;
  background: var(--app-surface);
  border-bottom: 1px solid var(--app-border);
  user-select: none;
  flex-shrink: 0;
}

.tabs-container {
  display: flex;
  align-items: flex-end;
  height: 100%;
  padding-left: 10px;
  gap: 3px;
}



.titlebar-drag-region { min-width: 16px; flex: 1; height: 100%; }
.titlebar-actions, .window-controls { display: flex; align-self: stretch; }
.titlebar-action, .window-control { display: grid; place-items: center; width: 40px; height: 100%; padding: 0; border: 0; background: transparent; color: var(--app-muted); cursor: pointer; }
.titlebar-language { display: inline-flex; align-items: center; align-self: stretch; gap: 5px; padding: 0 10px; border: 0; background: transparent; color: var(--app-muted); font: inherit; font-size: 12px; cursor: pointer; }
.titlebar-language:hover { background: var(--app-hover); color: var(--app-text); }
.sync-cloud-icon { position: relative; display: grid; place-items: center; width: 17px; height: 17px; }
.sync-status-dot { position: absolute; top: -3px; right: -5px; width: 6px; height: 6px; border: 1px solid var(--app-surface); border-radius: 50%; }
.sync-status-dot.is-info { background: #38bdf8; }
.sync-status-dot.is-success { background: #22c55e; }
.sync-status-dot.is-warning { background: #eab308; }
.sync-status-dot.is-error { background: #ef4444; }
.titlebar-action:hover, .titlebar-action.active, .window-control:hover { background: var(--app-hover); color: var(--app-text); }
.quick-sync-panel { display: grid; width: 280px; gap: 12px; }
.quick-sync-settings { display: inline-flex; align-items: center; gap: 5px; padding: 0; border: 0; background: transparent; color: var(--n-text-color-2); font: inherit; font-size: 12px; cursor: pointer; }
.quick-sync-settings:hover { color: var(--n-text-color); }
.quick-sync-settings.enabled { color: var(--n-success-color); }
.quick-sync-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.quick-sync-actions .n-button { width: 100%; }
.quick-sync-panel header { display: flex; align-items: center; justify-content: space-between; color: var(--n-text-color); }
.quick-sync-panel header span { color: var(--n-text-color-2); font-size: 12px; }
.quick-sync-panel header span.enabled { color: var(--n-success-color); }
.quick-sync-panel p { margin: 0; color: var(--n-text-color-2); font-size: 13px; line-height: 1.55; }
.quick-sync-provider { display: flex; align-items: center; gap: 8px; color: var(--n-text-color); font-size: 13px; font-weight: 600; }
.quick-sync-provider svg { color: var(--n-primary-color); }
.quick-sync-panel dl { display: grid; gap: 8px; margin: 0; }
.quick-sync-panel dl div { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; }
.quick-sync-panel dt { color: var(--n-text-color-2); }
.quick-sync-panel dd { max-width: 164px; margin: 0; overflow: hidden; color: var(--n-text-color); text-align: right; text-overflow: ellipsis; white-space: nowrap; }
.quick-sync-panel :deep(.n-button--text-type) { justify-self: center; }
.minimize-icon { fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.6; }
.window-control.close:hover { background: #c94f62; color: #fff; }

.tab {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 10px;
  border-radius: 5px 5px 0 0;
  font-size: 12px;
  color: var(--app-muted);
  cursor: pointer;
  white-space: nowrap;

  transition: background 0.15s, color 0.15s;
}

.tab:hover {
  background: var(--app-hover);
  color: var(--app-text);
}

.tab.active {
  background: var(--app-hover);
  color: var(--app-text);
}



.home-tab {
  box-sizing: border-box;
  flex: 0 0 42px;
  width: 42px;
  padding: 0;
  justify-content: center;
  margin-right: 1px;
}

.tab-dot {
  width: 7px;
  height: 7px;
  margin-left: -3px;
  border-radius: 50%;
  background: #8ee49a;
  flex-shrink: 0;
}

.tab-title {
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  margin-left: 3px;
  border-radius: 3px;
  font-size: 14px;
  line-height: 1;
  color: #b8c0d0;
  opacity: 0.9;
  transition: opacity 0.15s;
}

.tab:hover .tab-close {
  opacity: 1;
}

.tab-close:hover {
  background: #585b70;
  opacity: 1 !important;
}

.new-tab {
  align-self: flex-end;
  width: 30px;
  height: 30px;
  padding: 0;
  justify-content: center;
  border-radius: 5px 5px 0 0;
  background: transparent !important;
  color: #8992a7;
}

.new-tab:hover {
  color: #d8deeb;
}

/* ==================== Terminal Toolbar ==================== */
.terminal-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 32px;
  padding: 0 12px;
  background: var(--app-surface);
  border-bottom: 1px solid var(--app-border);
  flex-shrink: 0;
}

.toolbar-left,
.toolbar-right {
  display: flex;
  align-items: center;
}

.toolbar-left {
  gap: 7px;
  color: var(--app-muted);
}

.toolbar-info {
  font-size: 11px;
  font-family: 'Cascadia Mono', 'Cascadia Code', Consolas, monospace;
}

.toolbar-stats {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-left: 8px;
  font-family: 'Cascadia Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 10px;
  color: var(--app-muted);
}

.stat-item {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  white-space: nowrap;
}

.stat-item :deep(svg) {
  color: #8994a8;
  stroke-width: 1.8;
}

.toolbar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: #8d97aa;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.toolbar-btn:hover,
.toolbar-btn.active {
  background: var(--app-hover);
  color: var(--app-text);
}

.toolbar-btn.active {
  box-shadow: inset 0 0 0 1px var(--app-border);
}

.transfer-control {
  position: relative;
}

.transfer-button {
  position: relative;
}

.transfer-notice {
  position: absolute;
  z-index: 190;
  top: 32px;
  right: 0;
  display: grid;
  grid-template-columns: auto auto;
  gap: 3px 10px;
  min-width: 220px;
  padding: 9px 10px;
  border: 1px solid var(--app-border);
  border-radius: 5px;
  background: var(--app-panel);
  box-shadow: 0 8px 20px var(--app-shadow);
  color: var(--app-text);
  font-size: 11px;
  white-space: nowrap;
}

.transfer-notice::before {
  position: absolute;
  top: -5px;
  right: 9px;
  width: 8px;
  height: 8px;
  border-top: 1px solid var(--app-border);
  border-left: 1px solid var(--app-border);
  background: var(--app-panel);
  content: '';
  transform: rotate(45deg);
}

.transfer-notice strong {
  color: #9fe7aa;
  font-weight: 500;
}

.transfer-notice span {
  overflow: hidden;
  max-width: 135px;
  color: var(--app-muted);
  text-overflow: ellipsis;
}

.transfer-notice button {
  grid-column: 1 / -1;
  justify-self: start;
  margin-top: 2px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--app-accent);
  font-size: 11px;
  cursor: pointer;
}

.transfer-notice button:hover {
  color: var(--app-text);
}

.transfer-badge {
  position: absolute;
  top: 3px;
  right: 3px;
  width: 6px;
  height: 6px;
  border: 1px solid #141923;
  border-radius: 50%;
  background: #ed476c;
}

.main-script-panel,
.main-transfer-panel {
  position: absolute;
  z-index: 180;
  top: 72px;
  right: 44px;
  width: min(396px, calc(100% - 16px));
}

.main-script-panel { right: 72px; }

/* ==================== Connection Info (deprecated) ==================== */
.connection-info {
  display: flex;
  align-items: center;
  height: 32px;
  padding: 0 16px;
  background: var(--app-surface);
  border-bottom: 1px solid var(--app-border);
  flex-shrink: 0;
}

.conn-user {
  font-size: 12px;
  color: var(--app-muted);
  font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace;
}

/* ==================== Content Area ==================== */
.content-area {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.terminal-tab {
  position: absolute;
  inset: 0;
}

/* ==================== Home ==================== */
.home-content {
  height: 100%;
}

.home-layout {
  display: flex;
  height: 100%;
  overflow: hidden;
}

.sidebar {
  width: 200px;
  background: var(--app-elevated);
  border-right: 1px solid var(--app-border);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.sidebar-logo {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px;
  border-bottom: 1px solid var(--app-border);
}

.logo-icon {
  width: 38px;
  height: 38px;
  object-fit: contain;
}

.logo-text {
  font-size: 17px;
  font-weight: 600;
  color: var(--app-text);
}

.sidebar-nav {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  padding: 8px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 8px;
  color: var(--app-muted);
  cursor: pointer;
  transition: all 0.15s;
  font-size: 14px;
}

.nav-item:hover {
  background: var(--app-hover);
  color: var(--app-text);
}

.nav-item.active {
  background: var(--app-hover);
  color: var(--app-text);
}

.sidebar-bottom {
  padding: 8px;
  border-top: 1px solid var(--app-border);
}

.main-content {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
  background: var(--app-base);
}

.content-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.header-left {
  display: flex;
  align-items: baseline;
  gap: 12px;
}

.header-left h2 {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--app-text);
}

.host-count {
  font-size: 13px;
  color: var(--app-muted);
}

.host-group {
  margin-bottom: 28px;
}

.group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 14px;
}

.group-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--app-text);
}

.group-count {
  font-size: 12px;
  color: var(--app-muted);
}

.host-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 8px;
}

.host-card {
  cursor: pointer;
  user-select: none;
}

.host-meta-row {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 10px;
}

.host-meta {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  line-height: 16px;
  color: color-mix(in srgb, var(--app-muted) 82%, var(--app-text));
}

.host-meta.location { display: flex; align-items: center; gap: 4px; }
.host-meta.location svg { flex: 0 0 auto; color: var(--app-accent); }

.host-info-refresh {
  display: flex;
  align-items: center;
  height: 16px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--app-muted);
  font: inherit;
  font-size: 11px;
  line-height: 16px;
  cursor: pointer;
}

.host-info-refresh svg {
  flex: 0 0 auto;
  margin-right: 4px;
}

.host-info-refresh:hover:not(:disabled) {
  color: var(--app-accent);
}

.host-info-refresh:disabled {
  cursor: default;
  opacity: .65;
}

.host-info-refresh .is-spinning {
  animation: host-info-refresh-spin .8s linear infinite;
}

@keyframes host-info-refresh-spin {
  to { transform: rotate(360deg); }
}

.host-actions {
  display: flex;
  align-items: center;
  gap: 1px;
}

.host-actions :deep(.n-button) { width: 22px; height: 22px; padding: 0; color: var(--app-muted); }
.host-actions :deep(.n-button .n-button__icon) { margin: 0; }
.host-actions :deep(.n-button:hover) { color: var(--app-text); }
.host-actions :deep(.n-button--error-type:hover) { color: #ef4444; }

@media (max-width: 760px) {
  .main-content { padding: 20px 16px; }
  .host-grid { grid-template-columns: minmax(0, 1fr); }
}

.host-icon-picker { display: grid; grid-template-columns: repeat(8, 30px); gap: 5px; }
.host-icon-picker button { display: grid; width: 30px; height: 30px; place-items: center; padding: 0; border: 1px solid var(--app-border); border-radius: 4px; background: var(--app-surface); color: var(--app-muted); cursor: pointer; }
.host-icon-picker button:hover { color: var(--app-text); background: var(--app-hover); }
.host-icon-picker button.selected { border-color: var(--app-accent); background: var(--app-selection); color: var(--app-accent); }
.host-color-picker { display: flex; flex-wrap: wrap; gap: 7px; }
.host-color-picker button { width: 22px; height: 22px; padding: 0; border: 2px solid transparent; border-radius: 50%; background: var(--picker-color); box-shadow: 0 0 0 1px var(--app-border); cursor: pointer; }
.host-color-picker button.selected { border-color: var(--app-surface); box-shadow: 0 0 0 2px var(--picker-color); }

/* SFTP 右侧面板 */
.sftp-panel {
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  width: 400px;
  display: flex;
  flex-direction: column;
  background: var(--app-base);
  border-left: 1px solid var(--app-border);
  box-shadow: -4px 0 16px var(--app-shadow);
  z-index: 100;
  overflow: hidden;
}

.sftp-resize-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  left: -4px;
  width: 8px;
  cursor: col-resize;
  z-index: 1;
}

.sftp-resize-handle:hover {
  background: var(--app-accent);
}



.sftp-panel-body {
  flex: 1;
  overflow: hidden;
}

.ai-panel { border-left-color: var(--app-border); }
.toolbar-btn.active { color: var(--app-accent); background: var(--app-hover); }

.settings-overlay {
  position: absolute;
  inset: 0;
  z-index: 500;
  display: grid;
  padding: 20px;
  background: var(--app-overlay);
  place-items: center;
}

.settings-window {
  display: flex;
  width: min(980px, calc(100vw - 40px));
  height: min(720px, calc(100vh - 40px));
  min-width: 0;
  max-height: 100%;
  flex-direction: column;
  color: var(--app-text);
  background: var(--app-base);
  border: 1px solid var(--app-border);
  border-radius: 10px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
  overflow: hidden;
}

.settings-titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 60px;
  padding: 0 18px;
  border-bottom: 1px solid var(--app-border);
}

.settings-titlebar h2,
.settings-content h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--app-text);
}

.settings-close {
  display: grid;
  width: 32px;
  height: 32px;
  padding: 0;
  color: var(--app-muted);
  background: transparent;
  border: 0;
  border-radius: 6px;
  cursor: pointer;
  place-items: center;
}

.settings-close:hover { color: var(--app-text); background: var(--app-hover); }

.settings-body {
  display: flex;
  min-height: 0;
  flex: 1;
}

.settings-nav {
  width: 224px;
  padding: 14px 12px;
  background: var(--app-surface);
  border-right: 1px solid var(--app-border);
}

.settings-nav button {
  display: flex;
  align-items: center;
  width: 100%;
  gap: 10px;
  margin-bottom: 3px;
  padding: 10px 12px;
  color: var(--app-muted);
  font: inherit;
  font-size: 14px;
  text-align: left;
  background: transparent;
  border: 0;
  border-radius: 7px;
  cursor: pointer;
}

.settings-nav button:hover { color: var(--app-text); background: var(--app-hover); }
.settings-nav button.active { color: var(--app-text); background: var(--app-hover); font-weight: 500; }
.settings-nav button.active svg { color: var(--app-accent); }

.settings-content {
  width: min(760px, 100%);
  padding: 32px 24px;
  overflow-y: auto;
}

.settings-content h3 { margin-bottom: 18px; }

.settings-panel {
  margin-bottom: 14px;
  padding: 4px 16px;
  background: var(--app-surface);
  border: 1px solid var(--app-border);
  border-radius: 10px;
}

.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 70px;
  gap: 18px;
  border-bottom: 1px solid var(--app-border);
}

.settings-row:last-child { border-bottom: 0; }
.settings-row strong, .sync-intro strong, .empty-settings strong { font-size: 14px; color: var(--app-text); }
.settings-row p, .sync-intro p, .empty-settings p { margin: 4px 0 0; font-size: 12px; color: var(--app-muted); }
.settings-value { flex-shrink: 0; font-size: 12px; color: var(--app-accent); }
.diagnostic-export-confirmation { display: grid; gap: 8px; }
.diagnostic-export-confirmation p { margin: 0 0 8px; color: var(--app-muted); font-size: 13px; line-height: 1.6; }
.diagnostic-export-confirmation :deep(.n-alert) { margin-top: 4px; line-height: 1.6; }

.sync-intro {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 16px;
}

.sync-intro > svg { flex: 0 0 auto; color: var(--app-accent); }
.password-settings { padding: 16px; }
.password-settings :deep(.n-form-item) { margin-bottom: 12px; }
.password-settings :deep(.n-form-item-label) { color: var(--app-muted) !important; }
.password-settings :deep(.n-input) {
  --n-color: var(--app-base) !important;
  --n-color-focus: var(--app-base) !important;
  --n-border: 1px solid var(--app-border) !important;
  --n-border-focus: 1px solid var(--app-accent) !important;
  --n-text-color: var(--app-text) !important;
  --n-placeholder-color: var(--app-muted) !important;
  --n-icon-color: var(--app-muted) !important;
  --n-icon-color-hover: var(--app-text) !important;
}
.password-settings :deep(.n-input-wrapper) { background: var(--app-base) !important; }
.password-settings :deep(.n-input__input-el),
.password-settings :deep(.n-input__placeholder) { color: var(--app-text) !important; }
.password-settings :deep(.n-input__placeholder) { opacity: 1; color: var(--app-muted) !important; }
.password-settings :deep(.n-button) { font-weight: 600; }
.settings-actions { display: flex; justify-content: flex-end; margin-top: 4px; }

.empty-settings {
  display: flex;
  min-height: 150px;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
}

.empty-settings > svg { margin-bottom: 10px; color: var(--app-accent); }

@media (max-width: 640px) {
  .settings-overlay { padding: 10px; }
  .settings-window { width: calc(100vw - 20px); height: calc(100vh - 20px); }
  .settings-nav { width: 154px; }
  .settings-content { padding: 24px 16px; }
  .settings-nav button { padding: 10px 8px; }
}
</style>
