<script setup lang="ts">
import { defineAsyncComponent, ref, computed, onBeforeUnmount, onMounted, nextTick, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Cloud, Copy, Cpu, Download, HardDrive, ListFilter, MemoryStick, Minus, Moon, Sparkles, Square, Sun, Upload, X, Zap } from '@lucide/vue'
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
  NEmpty,
  NGlobalStyle,
  type GlobalThemeOverrides,
} from 'naive-ui'
import { useVaultStore } from './stores/vault'
import { useSessionStore } from './stores/session'
import { useTransferStore } from './stores/transfer'
const Terminal = defineAsyncComponent(() => import('./components/Terminal.vue'))
const ConnectionDialog = defineAsyncComponent(() => import('./components/ConnectionDialog.vue'))
const KeysView = defineAsyncComponent(() => import('./components/KeysView.vue'))
const SftpView = defineAsyncComponent(() => import('./components/SftpView.vue'))
const AiChatPanel = defineAsyncComponent(() => import('./components/AiChatPanel.vue'))
const AiSettings = defineAsyncComponent(() => import('./components/AiSettings.vue'))
const TransferPanel = defineAsyncComponent(() => import('./components/TransferPanel.vue'))
const PermissionsDialog = defineAsyncComponent(() => import('./components/PermissionsDialog.vue'))
const ActionDialog = defineAsyncComponent(() => import('./components/ActionDialog.vue'))
import type { SshProfileView, CreateProfileRequest } from './types'

const vaultStore = useVaultStore()
const sessionStore = useSessionStore()
const transferStore = useTransferStore()
const appWindow = getCurrentWindow()
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
const isMaximized = ref(false)
const transferPanelRef = ref<HTMLElement | null>(null)
const transferButtonRef = ref<HTMLButtonElement | null>(null)
const transferNoticeVisible = ref(false)
let transferNoticeTimer: ReturnType<typeof setTimeout> | null = null

function toggleTheme() {
  isDarkTheme.value = !isDarkTheme.value
  localStorage.setItem('my-ssh-theme', isDarkTheme.value ? 'dark' : 'light')
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
})

const authOptions = [
  { label: '密码认证', value: 'password' },
  { label: '密钥认证', value: 'key' },
  { label: '证书认证', value: 'certificate' },
]

const isEditing = ref(false)

const groupedProfiles = computed(() => {
  const groups = new Map<string, SshProfileView[]>()
  for (const p of vaultStore.profiles) {
    const group = p.group_name || '默认分组'
    if (!groups.has(group)) groups.set(group, [])
    groups.get(group)!.push(p)
  }
  return groups
})

// --- Lifecycle ---
const activeView = ref<'hosts' | 'keys'>('hosts')

function closeTransfersOnOutsideClick(event: PointerEvent) {
  const target = event.target as Node
  if (transferPanelRef.value?.contains(target) || transferButtonRef.value?.contains(target)) return
  transferPanelOpen.value = false
}

function showTransferNotice() {
  if (transferNoticeTimer) clearTimeout(transferNoticeTimer)
  transferNoticeVisible.value = true
  transferNoticeTimer = setTimeout(() => { transferNoticeVisible.value = false }, 3_500)
}

function openTransfers() {
  transferPanelOpen.value = !transferPanelOpen.value
  transferNoticeVisible.value = false
  if (transferPanelOpen.value) transferSeenCount.value = transferStore.tasks.length
}

onMounted(async () => {
  isMaximized.value = await appWindow.isMaximized()
  await Promise.all([vaultStore.init(), transferStore.initialize()])
  document.addEventListener('pointerdown', closeTransfersOnOutsideClick)
})

watch(activeView, (view) => {
  if (view === 'keys') void vaultStore.loadKeys()
})

watch(() => form.value.auth_type, (authType) => {
  if (authType !== 'password') void vaultStore.loadKeys()
})

watch(() => transferStore.tasks.length, (count, previousCount) => {
  if (count > previousCount) showTransferNotice()
})

// --- Lifecycle ---
// --- Profile CRUD ---
function openCreateForm() {
  isEditing.value = false
  editingProfile.value = null
  form.value = {
    name: '',
    host: '',
    port: 22,
    username: 'root',
    auth_type: 'password',
    credential: '',
    key_id: undefined,
    group_name: '',
  }
  showForm.value = true
}

function openEditForm(profile: SshProfileView) {
  isEditing.value = true
  editingProfile.value = profile
  form.value = {
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    auth_type: profile.auth_type,
    credential: '',
    key_id: profile.key_id || undefined,
    group_name: profile.group_name || '',
  }
  showForm.value = true
}

async function handleFormSubmit() {
  console.log('[Form] Submit clicked', JSON.stringify(form.value))
  if (!form.value.name || !form.value.host || !form.value.username) {
    console.log('[Form] Validation failed: missing required fields')
    return
  }

  // 验证认证方式
  if (form.value.auth_type === 'password') {
    if (!form.value.credential && !isEditing.value) {
      console.log('[Form] Validation failed: no credential')
      return
    }
  } else {
    if (!form.value.key_id && !isEditing.value) {
      console.log('[Form] Validation failed: no key_id')
      return
    }
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
  }

  console.log('[Form] Sending data:', JSON.stringify(data))

  if (isEditing.value && editingProfile.value) {
    await vaultStore.updateProfile(editingProfile.value.id, data)
  } else {
    const result = await vaultStore.createProfile(data)
    console.log('[Form] createProfile result:', result)
    console.log('[Form] vaultStore.error:', vaultStore.error)
  }
  showForm.value = false
  editingProfile.value = null
}

async function handleDeleteProfile(id: string) {
  await vaultStore.deleteProfile(id)
}

// --- Connection ---
const terminalRefs = ref<Record<string, any>>({})
const activeTerminalInfo = ref<{ host: string; port: number; username: string } | null>(null)

// --- Connection dialog ---
const connDialogVisible = ref(false)
const connDialogStatus = ref<'connecting' | 'authenticating' | 'success' | 'error'>('connecting')
const connDialogInfo = ref<{ host: string; port: number; username: string; profileName: string }>({ host: '', port: 22, username: '', profileName: '' })
const connDialogError = ref('')
const pendingProfile = ref<SshProfileView | null>(null)

async function handleConnect(profile: SshProfileView) {
  const existingTab = sessionStore.tabs.find(t => t.profileId === profile.id)
  if (existingTab) {
    sessionStore.setActiveTab(existingTab.sessionId)
    updateTerminalInfo(existingTab.sessionId)
    await nextTick()
    terminalRefs.value[existingTab.sessionId]?.triggerResize()
    return
  }

  // 显示连接弹窗
  pendingProfile.value = profile
  connDialogInfo.value = {
    host: profile.host,
    port: profile.port,
    username: profile.username,
    profileName: profile.name,
  }
  connDialogStatus.value = 'connecting'
  connDialogError.value = ''
  sessionStore.error = null
  connDialogVisible.value = true

  // 模拟进度
  await new Promise(r => setTimeout(r, 300))
  if (!connDialogVisible.value) return
  connDialogStatus.value = 'authenticating'

  // 发起连接
  activeTerminalInfo.value = { host: profile.host, port: profile.port, username: profile.username }
  const sessionId = await sessionStore.connect(profile.id, profile.name)

  if (sessionId) {
    connDialogStatus.value = 'success'
    await nextTick()
    await new Promise(resolve => setTimeout(resolve, 200))
    terminalRefs.value[sessionId]?.triggerResize()
  } else {
    connDialogStatus.value = 'error'
    connDialogError.value = sessionStore.error || '连接失败'
  }
}

async function handleRetry() {
  connDialogVisible.value = false
  if (pendingProfile.value) {
    await handleConnect(pendingProfile.value)
  }
}

function handleCloseConnDialog() {
  connDialogVisible.value = false
  if (pendingProfile.value) {
    const tab = sessionStore.tabs.find(t => t.profileId === pendingProfile.value!.id)
    if (tab) {
      sessionStore.closeTab(tab.sessionId)
    }
  }
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
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  document.removeEventListener('pointerdown', closeTransfersOnOutsideClick)
})

// --- Settings ---
const showSettings = ref(false)
const settingsSection = ref<'terminal' | 'ai' | 'sync' | 'system'>('terminal')
const changingPassword = ref(false)

function openSettings() {
  settingsSection.value = 'terminal'
  showSettings.value = true
}

watch(showSettings, (visible) => {
  if (visible && settingsSection.value === 'sync' && vaultStore.isDefaultPassword === null) {
    void vaultStore.loadDefaultPasswordStatus()
  }
})

watch(settingsSection, (section) => {
  if (section === 'sync' && vaultStore.isDefaultPassword === null) {
    void vaultStore.loadDefaultPasswordStatus()
  }
})
const pwdForm = ref({ oldPassword: '', newPassword: '', confirmPassword: '' })
const pwdError = ref('')

async function handleChangePassword() {
  pwdError.value = ''
  if (!pwdForm.value.newPassword) {
    pwdError.value = '请输入新密码'
    return
  }
  if (pwdForm.value.newPassword.length < 6) {
    pwdError.value = '新密码至少 6 位'
    return
  }
  if (pwdForm.value.newPassword !== pwdForm.value.confirmPassword) {
    pwdError.value = '两次输入的新密码不一致'
    return
  }

  const oldPwd = vaultStore.isDefaultPassword === true ? 'LuckyMJJ' : pwdForm.value.oldPassword
  if (!oldPwd) {
    pwdError.value = '请输入旧密码'
    return
  }

  changingPassword.value = true
  const success = await vaultStore.changePassword(oldPwd, pwdForm.value.newPassword)
  changingPassword.value = false

  if (success) {
    pwdForm.value = { oldPassword: '', newPassword: '', confirmPassword: '' }
    showSettings.value = false
  } else {
    pwdError.value = vaultStore.error || '修改失败，请检查旧密码'
  }
}
</script>

<template>
  <n-config-provider :theme="naiveTheme" :theme-overrides="naiveThemeOverrides">
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
              <span class="tab-dot" title="已连接" />
              <span class="tab-close" @click.stop="handleCloseTab(tab.sessionId)">×</span>
            </div>

            <div class="tab new-tab" @click="sessionStore.activeTabId = null; activeTerminalInfo = null">
              <span>+</span>
            </div>
          </div>
          <div class="titlebar-drag-region" data-tauri-drag-region @mousedown="startWindowDrag" />
          <div class="titlebar-actions" aria-label="应用功能">
            <button class="titlebar-action" title="云同步" aria-label="云同步"><Cloud :size="17" /></button>
            <button class="titlebar-action" :title="isDarkTheme ? '切换为浅色主题' : '切换为深色主题'" aria-label="切换主题" @click="toggleTheme"><Sun v-if="isDarkTheme" :size="17" /><Moon v-else :size="17" /></button>
          </div>
          <div class="window-controls">
            <button class="window-control" title="最小化" @click="minimizeWindow"><Minus :size="17" /></button>
            <button class="window-control" :title="isMaximized ? '还原' : '最大化'" @click="toggleMaximizeWindow"><Copy v-if="isMaximized" :size="14" /><Square v-else :size="13" /></button>
            <button class="window-control close" title="关闭" @click="closeWindow"><X :size="17" /></button>
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
                    <span>主机</span>
                  </div>
                  <div class="nav-item" :class="{ active: activeView === 'keys' }" @click="activeView = 'keys'">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>
                    </svg>
                    <span>密钥</span>
                  </div>
                </div>

                <div class="sidebar-bottom">
                  <div class="nav-item" @click="openSettings">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <circle cx="12" cy="12" r="3"/>
                      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                    <span>设置</span>
                  </div>
                </div>
              </div>

              <!-- Main content -->
              <div class="main-content">
                <!-- Keys view -->
                <KeysView v-if="activeView === 'keys'" />

                <!-- Hosts view -->
                <template v-else>
                <div class="content-header">
                  <div class="header-left">
                    <h2>主机</h2>
                    <span class="host-count">{{ vaultStore.profiles.length }} 条</span>
                  </div>
                  <n-button type="primary" @click="openCreateForm">
                    <template #icon>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <line x1="12" y1="5" x2="12" y2="19"/>
                        <line x1="5" y1="12" x2="19" y2="12"/>
                      </svg>
                    </template>
                    新建主机
                  </n-button>
                </div>



                <n-empty v-if="vaultStore.profiles.length === 0 && !vaultStore.loading" description="暂无主机" style="padding: 60px 0">
                  <template #extra>
                    <n-button type="primary" @click="openCreateForm">创建第一个主机</n-button>
                  </template>
                </n-empty>

                <div v-else v-for="[group, items] in groupedProfiles" :key="group" class="host-group">
                  <div class="group-header">
                    <span class="group-name">{{ group }}</span>
                    <span class="group-count">{{ items.length }} 条</span>
                  </div>

                  <div class="host-grid">
                    <div
                      v-for="profile in items"
                      :key="profile.id"
                      class="host-card"
                      @dblclick="handleConnect(profile)"
                    >
                      <div class="host-icon" :class="profile.auth_type">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
                          <line x1="8" y1="21" x2="16" y2="21"/>
                          <line x1="12" y1="17" x2="12" y2="21"/>
                        </svg>
                      </div>
                      <div class="host-info">
                        <div class="host-name">{{ profile.name }}</div>
                        <div class="host-detail">{{ profile.username }}@{{ profile.host }}</div>
                      </div>
                      <div class="host-actions" @click.stop>
                          <n-button size="tiny" quaternary @click="openEditForm(profile)" style="color: #a6adc8">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                            </svg>
                          </n-button>
                          <n-popconfirm @positive-click="handleDeleteProfile(profile.id)">
                            <template #trigger>
                              <n-button size="tiny" quaternary type="error" style="color: #f38ba8">
                                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                  <polyline points="3 6 5 6 21 6"/>
                                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                                </svg>
                              </n-button>
                            </template>
                            确定删除 "{{ profile.name }}"？
                          </n-popconfirm>
                        </div>
                    </div>
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
                />
              </template>
            </div>
          </div>
        </div>

        <!-- Profile form modal -->
        <n-modal
          v-model:show="showForm"
          :title="isEditing ? '编辑主机' : '新建主机'"
          preset="card"
          style="width: 500px"
        >
          <n-form :model="form" label-placement="left" label-width="80">
            <n-form-item label="名称" required>
              <n-input v-model:value="form.name" placeholder="My Server" />
            </n-form-item>
            <n-form-item label="主机" required>
              <n-input v-model:value="form.host" placeholder="192.168.1.100" />
            </n-form-item>
            <n-form-item label="端口">
              <n-input-number v-model:value="form.port" :min="1" :max="65535" style="width: 120px" />
            </n-form-item>
            <n-form-item label="用户名" required>
              <n-input v-model:value="form.username" placeholder="root" />
            </n-form-item>
            <n-form-item label="认证方式">
              <n-select v-model:value="form.auth_type" :options="authOptions" />
            </n-form-item>
            <n-form-item v-if="form.auth_type === 'password'" label="密码" required>
              <n-input
                v-model:value="form.credential"
                type="password"
                placeholder="请输入密码"
                show-password-on="click"
              />
            </n-form-item>
            <n-form-item v-if="form.auth_type !== 'password'" label="密钥" required>
              <n-select
                :value="form.key_id"
                :options="vaultStore.sshKeys.map(k => ({ label: k.name, value: k.id }))"
                placeholder="选择已配置的密钥"
                @update:value="(val: string) => { form.key_id = val }"
              />
              <n-button v-if="vaultStore.sshKeys.length === 0" size="small" type="primary" style="margin-left: 8px" @click="activeView = 'keys'; showForm = false">
                去创建
              </n-button>
            </n-form-item>
            <n-form-item label="分组">
              <n-input v-model:value="form.group_name" placeholder="可选，如: 生产环境" />
            </n-form-item>
          </n-form>

          <template #footer>
            <n-space justify="end">
              <n-button @click="showForm = false">取消</n-button>
              <n-button type="primary" :loading="vaultStore.loading" @click="handleFormSubmit">
                {{ isEditing ? '保存' : '创建' }}
              </n-button>
            </n-space>
          </template>
        </n-modal>
        <!-- Connection dialog -->
        <ConnectionDialog
          v-model:show="connDialogVisible"
          :host="connDialogInfo.host"
          :port="connDialogInfo.port"
          :username="connDialogInfo.username"
          :profile-name="connDialogInfo.profileName"
          :status="connDialogStatus"
          :error="connDialogError"
          :dark="isDarkTheme"
          @retry="handleRetry"
          @close="handleCloseConnDialog"
        />


        <div v-if="showSettings" class="settings-overlay" role="dialog" aria-modal="true" aria-label="设置">
          <section class="settings-window">
            <header class="settings-titlebar">
              <h2>设置</h2>
              <button class="settings-close" aria-label="关闭设置" @click="showSettings = false"><X :size="18" /></button>
            </header>
            <div class="settings-body">
              <nav class="settings-nav" aria-label="设置分类">
                <button :class="{ active: settingsSection === 'terminal' }" @click="settingsSection = 'terminal'">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="4" width="18" height="15" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/></svg>
                  终端
                </button>
                <button :class="{ active: settingsSection === 'ai' }" @click="settingsSection = 'ai'"><Sparkles :size="16" />AI</button>
                <button :class="{ active: settingsSection === 'sync' }" @click="settingsSection = 'sync'"><Cloud :size="16" />云同步</button>
                <button :class="{ active: settingsSection === 'system' }" @click="settingsSection = 'system'">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="3" width="18" height="18" rx="3"/><path d="M8 8h8M8 12h8M8 16h5"/></svg>
                  系统
                </button>
              </nav>
              <main class="settings-content">
                <template v-if="settingsSection === 'terminal'">
                  <h3>终端</h3>
                  <div class="settings-panel">
                    <div class="settings-row"><div><strong>终端渲染</strong><p>使用 WebGL 加速渲染 SSH 终端。</p></div><span class="settings-value">已启用</span></div>
                    <div class="settings-row"><div><strong>滚动缓冲区</strong><p>保留最近 5000 行终端输出。</p></div><span class="settings-value">5000 行</span></div>
                  </div>
                </template>
                <AiSettings v-else-if="settingsSection === 'ai'" />
                <template v-else-if="settingsSection === 'sync'">
                  <h3>云同步</h3>
                  <div class="settings-panel sync-intro"><Cloud :size="20" /><div><strong>同步主密码</strong><p>主密码仅用于加密云同步数据，不影响本地 SSH 凭证。</p></div></div>
                  <div class="settings-panel password-settings">
                    <n-alert v-if="pwdError" type="error">{{ pwdError }}</n-alert>
                    <n-form label-placement="left" label-width="88" size="small">
                      <n-form-item label="当前密码"><n-input :value="vaultStore.isDefaultPassword === true ? 'LuckyMJJ' : pwdForm.oldPassword" type="password" show-password-on="click" :placeholder="vaultStore.isDefaultPassword === null ? '正在检测当前主密码...' : vaultStore.isDefaultPassword ? '默认密码（自动填入）' : '请输入当前主密码'" :disabled="vaultStore.isDefaultPassword === true || vaultStore.isDefaultPassword === null" @update:value="(val: string) => pwdForm.oldPassword = val" /></n-form-item>
                      <n-form-item label="新密码"><n-input v-model:value="pwdForm.newPassword" type="password" show-password-on="click" placeholder="至少 6 位" /></n-form-item>
                      <n-form-item label="确认密码"><n-input v-model:value="pwdForm.confirmPassword" type="password" show-password-on="click" placeholder="再次输入新密码" /></n-form-item>
                    </n-form>
                    <div class="settings-actions"><n-button type="primary" size="small" :loading="changingPassword" @click="handleChangePassword">更新主密码</n-button></div>
                  </div>
                </template>
                <template v-else>
                  <h3>系统</h3>
                  <div class="settings-panel">
                    <div class="settings-row"><div><strong>MJJSSH</strong><p>多窗口 SSH 客户端</p></div><span class="settings-value">v0.1.0</span></div>
                    <div class="settings-row"><div><strong>界面主题</strong><p>使用应用当前的{{ isDarkTheme ? '深色' : '亮色' }}配色方案。</p></div><span class="settings-value">{{ isDarkTheme ? '深色' : '亮色' }}</span></div>
                  </div>
                </template>
              </main>
            </div>
          </section>
        </div>

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
.titlebar-action, .window-control { display: grid; place-items: center; width: 40px; padding: 0; border: 0; background: transparent; color: var(--app-muted); cursor: pointer; }
.titlebar-action:hover, .window-control:hover { background: var(--app-hover); color: var(--app-text); }
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
  padding: 0 8px;
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
  align-self: center;
  height: 28px;
  padding: 0 8px;
  border-radius: 4px;
  background: transparent !important;
  font-size: 17px;
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

.main-transfer-panel {
  position: absolute;
  z-index: 180;
  top: 72px;
  right: 44px;
  width: min(396px, calc(100% - 16px));
}

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
  flex: 1;
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
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 12px;
}

.host-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  background: var(--app-surface);
  border: 1px solid var(--app-border);
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.2s;
  user-select: none;
}

.host-card:hover {
  background: var(--app-elevated);
  border-color: var(--app-border);
  transform: translateY(-1px);
}

.host-icon {
  width: 42px;
  height: 42px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: #fff;
}

.host-icon.password {
  background: linear-gradient(135deg, #f38ba8, #eba0ac);
}

.host-icon.key {
  background: linear-gradient(135deg, #f9e2af, #fab387);
}

.host-icon.certificate {
  background: linear-gradient(135deg, #a6e3a1, #94e2d5);
}

.host-info {
  flex: 1;
  min-width: 0;
}

.host-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--app-text);
  margin-bottom: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.host-detail {
  font-size: 12px;
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.host-actions {
  display: flex;
  gap: 2px;
  opacity: 0.4;
  transition: opacity 0.15s;
}

.host-card:hover .host-actions {
  opacity: 1;
}


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
