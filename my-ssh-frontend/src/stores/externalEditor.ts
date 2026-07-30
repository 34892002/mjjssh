import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

import type { ExternalEditSession, ExternalEditSessionStatus, UploadExternalEditResult } from '../types'

export const useExternalEditorStore = defineStore('externalEditor', () => {
  const sessions = ref<ExternalEditSession[]>([])
  const loadingEditIds = ref(new Set<string>())
  const isCleaning = ref(false)
  const errors = ref<Record<string, string>>({})

  const sessionsById = computed(() => new Map(sessions.value.map((session) => [session.editId, session])))

  function replaceSession(session: ExternalEditSession) {
    const index = sessions.value.findIndex((item) => item.editId === session.editId)
    if (index === -1) sessions.value.unshift(session)
    else sessions.value[index] = { ...sessions.value[index], ...session }
  }

  function setLoading(editId: string, loading: boolean) {
    const next = new Set(loadingEditIds.value)
    if (loading) next.add(editId)
    else next.delete(editId)
    loadingEditIds.value = next
  }

  function setError(editId: string, error?: unknown) {
    const next = { ...errors.value }
    if (error) next[editId] = String(error)
    else delete next[editId]
    errors.value = next
  }

  async function createAndOpen(sessionId: string, path: string) {
    const session = await invoke<ExternalEditSession>('create_external_edit_session', { sessionId, path })
    replaceSession(session)
    try {
      await invoke('open_external_edit_session', { editId: session.editId })
    } catch (error) {
      setError(session.editId, error)
      throw error
    }
    return session
  }

  async function open(editId: string) {
    setLoading(editId, true)
    try {
      await invoke('open_external_edit_session', { editId })
      setError(editId)
    } catch (error) {
      setError(editId, error)
      throw error
    } finally {
      setLoading(editId, false)
    }
  }

  async function refresh(editId: string) {
    try {
      const status = await invoke<ExternalEditSessionStatus>('get_external_edit_session_status', { editId })
      replaceSession(status)
      setError(editId)
      return status
    } catch (error) {
      setError(editId, error)
      throw error
    }
  }

  async function upload(editId: string, force = false) {
    setLoading(editId, true)
    try {
      const result = await invoke<UploadExternalEditResult>('upload_external_edit_session', { editId, force })
      const session = sessionsById.value.get(editId)
      if (result.kind === 'uploaded') {
        sessions.value = sessions.value.filter((session) => session.editId !== editId)
      } else if (session) {
        replaceSession({ ...session, status: 'conflict', version: result.currentVersion })
      }
      setError(editId)
      return result
    } catch (error) {
      setError(editId, error)
      throw error
    } finally {
      setLoading(editId, false)
    }
  }

  async function reload(editId: string) {
    setLoading(editId, true)
    try {
      const session = await invoke<ExternalEditSession>('reload_external_edit_session', { editId })
      replaceSession(session)
      setError(editId)
      return session
    } catch (error) {
      setError(editId, error)
      throw error
    } finally {
      setLoading(editId, false)
    }
  }

  async function clearAll() {
    isCleaning.value = true
    try {
      await invoke('clear_external_edit_sessions')
      sessions.value = []
      errors.value = {}
    } catch (error) {
      const message = String(error)
      errors.value = Object.fromEntries(sessions.value.map((session) => [session.editId, message]))
      throw error
    } finally {
      isCleaning.value = false
    }
  }

  async function discard(editId: string) {
    setLoading(editId, true)
    try {
      await invoke('discard_external_edit_session', { editId })
      sessions.value = sessions.value.filter((session) => session.editId !== editId)
      setError(editId)
    } catch (error) {
      setError(editId, error)
      throw error
    } finally {
      setLoading(editId, false)
    }
  }

  function sessionsForSession(sessionId: string) {
    return computed(() => sessions.value.filter((session) => session.sessionId === sessionId))
  }

  return {
    sessions,
    loadingEditIds,
    isCleaning,
    errors,
    sessionsForSession,
    createAndOpen,
    open,
    refresh,
    upload,
    reload,
    clearAll,
    discard,
  }
})
