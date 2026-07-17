import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionInfo } from '../types'

export interface TabInfo {
  sessionId: string
  profileId: string
  profileName: string
}

function generateId(): string {
  return crypto.randomUUID()
}

export const useSessionStore = defineStore('session', () => {
  const sessions = ref<SessionInfo[]>([])
  const tabs = ref<TabInfo[]>([])
  const activeTabId = ref<string | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const terminalReadyResolvers = new Map<string, () => void>()

  const activeTab = computed(() =>
    tabs.value.find((t) => t.sessionId === activeTabId.value) ?? null,
  )

  async function connect(profileId: string, profileName: string): Promise<string | null> {
    loading.value = true
    error.value = null
    const sessionId = generateId()
    try {
      // 先创建页签，等待 Terminal 注册 SSH 事件监听器后再发起连接。
      const terminalReady = new Promise<void>((resolve) => {
        terminalReadyResolvers.set(sessionId, resolve)
      })
      tabs.value.push({ sessionId, profileId, profileName })
      activeTabId.value = sessionId

      await Promise.race([
        terminalReady,
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error('Terminal initialization timed out')), 10_000)),
      ])
      terminalReadyResolvers.delete(sessionId)

      await invoke<string>('connect_ssh', { profileId, sessionId })

      await loadSessions()
      return sessionId
    } catch (e) {
      error.value = String(e)
      // 移除失败的 tab
      terminalReadyResolvers.delete(sessionId)
      const failedTab = tabs.value.find(t => t.profileId === profileId)
      if (failedTab) {
        const idx = tabs.value.indexOf(failedTab)
        tabs.value.splice(idx, 1)
        if (activeTabId.value === failedTab.sessionId) {
          activeTabId.value = tabs.value.length > 0 ? tabs.value[tabs.value.length - 1].sessionId : null
        }
      }
      return null
    } finally {
      loading.value = false
    }
  }

  async function disconnect(sessionId: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('disconnect_ssh', { sessionId })
      const idx = tabs.value.findIndex((t) => t.sessionId === sessionId)
      if (idx !== -1) tabs.value.splice(idx, 1)
      if (activeTabId.value === sessionId) {
        activeTabId.value = tabs.value.length > 0 ? tabs.value[tabs.value.length - 1].sessionId : null
      }
      await loadSessions()
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  function closeTab(sessionId: string) {
    terminalReadyResolvers.delete(sessionId)
    void disconnect(sessionId)
  }

  function notifyTerminalReady(sessionId: string) {
    terminalReadyResolvers.get(sessionId)?.()
  }

  function setActiveTab(sessionId: string) {
    activeTabId.value = sessionId
  }

  async function loadSessions() {
    try {
      sessions.value = await invoke<SessionInfo[]>('list_sessions')
    } catch (e) {
      error.value = String(e)
    }
  }

  async function writeData(sessionId: string, data: string): Promise<boolean> {
    try {
      const encoder = new TextEncoder()
      await invoke('write_ssh_data', {
        sessionId,
        data: Array.from(encoder.encode(data)),
      })
      return true
    } catch (e) {
      error.value = String(e)
      return false
    }
  }

  async function resize(sessionId: string, cols: number, rows: number): Promise<boolean> {
    try {
      await invoke('resize_ssh', { sessionId, cols, rows })
      return true
    } catch (e) {
      error.value = String(e)
      return false
    }
  }

  return {
    sessions,
    tabs,
    activeTabId,
    activeTab,
    loading,
    error,
    connect,
    disconnect,
    closeTab,
    notifyTerminalReady,
    setActiveTab,
    loadSessions,
    writeData,
    resize,
  }
})
