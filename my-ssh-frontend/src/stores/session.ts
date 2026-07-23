import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionInfo } from '../types'

export interface TabInfo {
  sessionId: string
  profileId: string
  profileName: string
}

export interface TerminalSelection {
  sessionId: string
  text: string
  lineCount: number
}

const TERMINAL_SELECTION_MAX_BYTES = 8 * 1024
const TERMINAL_SELECTION_TTL_MS = 2 * 60 * 1000

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
  const terminalSelections = new Map<string, TerminalSelection>()
  const terminalSelectionTimers = new Map<string, ReturnType<typeof setTimeout>>()

  function clearTerminalSelection(sessionId: string) {
    const timer = terminalSelectionTimers.get(sessionId)
    if (timer) clearTimeout(timer)
    terminalSelectionTimers.delete(sessionId)
    terminalSelections.delete(sessionId)
  }

  function rememberTerminalSelection(sessionId: string, text: string) {
    clearTerminalSelection(sessionId)
    if (!text || new TextEncoder().encode(text).byteLength > TERMINAL_SELECTION_MAX_BYTES) return

    const selection = {
      sessionId,
      text,
      lineCount: text.split(/\r?\n/).length,
    }
    terminalSelections.set(sessionId, selection)
    terminalSelectionTimers.set(sessionId, setTimeout(
      () => clearTerminalSelection(sessionId),
      TERMINAL_SELECTION_TTL_MS,
    ))
  }

  function consumeTerminalSelection(sessionId: string, text: string): TerminalSelection | null {
    const selection = terminalSelections.get(sessionId)
    if (!selection || selection.text !== text) return null
    clearTerminalSelection(sessionId)
    return selection
  }

  const activeTab = computed(() =>
    tabs.value.find((t) => t.sessionId === activeTabId.value) ?? null,
  )

  async function connect(profileId: string, profileName: string, reuseSessionId?: string): Promise<string | null> {
    loading.value = true
    error.value = null
    const sessionId = reuseSessionId ?? generateId()
    const isReconnect = Boolean(reuseSessionId)
    try {
      if (!isReconnect) {
        // Create the tab first so Terminal can subscribe before the SSH stream starts.
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
      } else {
        activeTabId.value = sessionId
      }

      await invoke<string>('connect_ssh', { profileId, sessionId })

      await loadSessions()
      return sessionId
    } catch (e) {
      error.value = String(e)
      terminalReadyResolvers.delete(sessionId)
      if (!isReconnect) {
        const failedTab = tabs.value.find((tab) => tab.sessionId === sessionId)
        if (failedTab) {
          const idx = tabs.value.indexOf(failedTab)
          tabs.value.splice(idx, 1)
          if (activeTabId.value === failedTab.sessionId) {
            activeTabId.value = tabs.value.length > 0 ? tabs.value[tabs.value.length - 1].sessionId : null
          }
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
      clearTerminalSelection(sessionId)
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
    rememberTerminalSelection,
    consumeTerminalSelection,
    clearTerminalSelection,
    setActiveTab,
    loadSessions,
    writeData,
    resize,
  }
})
