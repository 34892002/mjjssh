import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { RemoteEditorLanguage, RemoteFileVersion, RemoteTextEncoding, RemoteTextLineEnding } from '../types'

export type RemoteEditorStatus = 'saved' | 'dirty' | 'saving' | 'remote-changed' | 'error'

export interface RemoteEditorTab {
  id: string
  sessionId: string
  path: string
  content: string
  savedContent: string
  encoding: RemoteTextEncoding
  lineEnding: RemoteTextLineEnding
  language: RemoteEditorLanguage
  version: RemoteFileVersion
  containsNul: boolean
  status: RemoteEditorStatus
  error: string | null
}

export interface OpenRemoteEditorTab extends Omit<RemoteEditorTab, 'id' | 'savedContent' | 'status' | 'error'> {}

function normalizePath(path: string) {
  const segments = path.split('/').filter(Boolean)
  return `/${segments.join('/')}` || '/'
}

export function remoteEditorTabId(sessionId: string, path: string) {
  return `${sessionId}:${normalizePath(path)}`
}

export const useRemoteEditorStore = defineStore('remote-editor', () => {
  const tabs = ref<RemoteEditorTab[]>([])
  const activeTabId = ref<string | null>(null)
  const activeTab = computed(() => tabs.value.find((tab) => tab.id === activeTabId.value) ?? null)

  function openTab(tab: OpenRemoteEditorTab) {
    const id = remoteEditorTabId(tab.sessionId, tab.path)
    const existing = tabs.value.find((item) => item.id === id)
    if (existing) {
      activeTabId.value = id
      return existing
    }

    const opened: RemoteEditorTab = {
      ...tab,
      id,
      path: normalizePath(tab.path),
      savedContent: tab.content,
      status: 'saved',
      error: null,
    }
    tabs.value.push(opened)
    activeTabId.value = id
    return opened
  }

  function updateContent(id: string, content: string) {
    const tab = tabs.value.find((item) => item.id === id)
    if (!tab) return
    tab.content = content
    tab.status = content === tab.savedContent ? 'saved' : 'dirty'
    tab.error = null
  }

  function updateTab(id: string, update: Partial<Omit<RemoteEditorTab, 'id'>>) {
    const tab = tabs.value.find((item) => item.id === id)
    if (tab) Object.assign(tab, update)
  }

  function closeTab(id: string) {
    const index = tabs.value.findIndex((tab) => tab.id === id)
    if (index === -1) return
    tabs.value.splice(index, 1)
    if (activeTabId.value === id) activeTabId.value = tabs.value[index]?.id ?? tabs.value[index - 1]?.id ?? null
  }

  function closeSession(sessionId: string) {
    const ids = new Set(tabs.value.filter((tab) => tab.sessionId === sessionId).map((tab) => tab.id))
    tabs.value = tabs.value.filter((tab) => !ids.has(tab.id))
    if (activeTabId.value && ids.has(activeTabId.value)) activeTabId.value = tabs.value.at(-1)?.id ?? null
  }

  return { tabs, activeTabId, activeTab, openTab, updateContent, updateTab, closeTab, closeSession }
})
