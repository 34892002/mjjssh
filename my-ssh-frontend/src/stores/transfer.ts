import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type TransferDirection = 'upload' | 'download'
export type TransferStatus = 'queued' | 'transferring' | 'completed' | 'failed'

export interface TransferTask {
  id: string
  sessionId: string
  direction: TransferDirection
  name: string
  source: string
  destination: string
  totalBytes: number
  transferredBytes: number
  status: TransferStatus
  error?: string
  startedAt?: number
  completedAt?: number
  overwrite: boolean
}

const maxHistory = 100
const maxConcurrentPerSession = 2

export const useTransferStore = defineStore('transfer', () => {
  const tasks = ref<TransferTask[]>([])
  const downloadDirectory = ref('')
  let unlistenProgress: UnlistenFn | null = null

  const activeCountBySession = computed(() => {
    const counts = new Map<string, number>()
    for (const task of tasks.value) {
      if (task.status === 'transferring') counts.set(task.sessionId, (counts.get(task.sessionId) ?? 0) + 1)
    }
    return counts
  })

  async function initialize() {
    if (unlistenProgress) return
    downloadDirectory.value = await invoke<string>('get_default_download_directory')
    unlistenProgress = await listen<{ id: string; transferred_bytes: number; total_bytes: number }>('sftp-transfer-progress', ({ payload }) => {
      const task = tasks.value.find((item) => item.id === payload.id)
      if (!task) return
      task.transferredBytes = payload.transferred_bytes
      task.totalBytes = payload.total_bytes
    })
  }

  function enqueue(task: Omit<TransferTask, 'id' | 'status' | 'transferredBytes'>) {
    const item: TransferTask = {
      ...task,
      id: crypto.randomUUID(),
      status: 'queued',
      transferredBytes: 0,
    }
    tasks.value.unshift(item)
    if (tasks.value.length > maxHistory) tasks.value.splice(maxHistory)
    void schedule(item.sessionId)
  }

  function enqueueUpload(sessionId: string, localPaths: string[], remotePath: string, overwrite = false) {
    for (const localPath of localPaths) {
      const name = localPath.split(/[/\\\\]/).pop() || localPath
      enqueue({ sessionId, direction: 'upload', name, source: localPath, destination: remotePath, totalBytes: 0, overwrite })
    }
  }

  function enqueueDownload(sessionId: string, remotePath: string, name: string, totalBytes: number, overwrite = false) {
    enqueue({ sessionId, direction: 'download', name, source: remotePath, destination: downloadDirectory.value, totalBytes, overwrite })
  }

  async function schedule(sessionId: string) {
    while ((activeCountBySession.value.get(sessionId) ?? 0) < maxConcurrentPerSession) {
      const task = tasks.value.find((item) => item.sessionId === sessionId && item.status === 'queued')
      if (!task) return
      task.status = 'transferring'
      task.startedAt = Date.now()
      void run(task)
    }
  }

  async function run(task: TransferTask) {
    try {
      if (task.direction === 'upload') {
        await invoke('sftp_upload_file', {
          sessionId: task.sessionId,
          localPath: task.source,
          remotePath: task.destination,
          transferId: task.id,
          overwrite: task.overwrite,
        })
      } else {
        await invoke('sftp_download_file', {
          sessionId: task.sessionId,
          remotePath: task.source,
          localDirectory: task.destination,
          transferId: task.id,
          overwrite: task.overwrite,
        })
      }
      task.status = 'completed'
      task.transferredBytes = task.totalBytes || task.transferredBytes
      task.completedAt = Date.now()
    } catch (error) {
      task.status = 'failed'
      task.error = String(error)
      task.completedAt = Date.now()
    } finally {
      void schedule(task.sessionId)
    }
  }

  function clearCompleted(sessionId: string) {
    tasks.value = tasks.value.filter((task) => task.sessionId !== sessionId || !['completed', 'failed'].includes(task.status))
  }

  function dispose() {
    unlistenProgress?.()
    unlistenProgress = null
  }

  return { tasks, downloadDirectory, initialize, enqueueUpload, enqueueDownload, clearCompleted, dispose }
})
