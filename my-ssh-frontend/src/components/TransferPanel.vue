<script setup lang="ts">
import { computed, ref } from 'vue'
import { Check, Download, FolderOpen, LoaderCircle, Trash2, Upload, XCircle } from '@lucide/vue'
import { useTransferStore } from '../stores/transfer'

const props = withDefaults(defineProps<{ sessionId: string; standalone?: boolean }>(), { standalone: false })
const transferStore = useTransferStore()
const editingDirectory = ref(false)
const directoryInput = ref('')
const tasks = computed(() => transferStore.tasks.filter((task) => task.sessionId === props.sessionId))

function formatBytes(bytes: number) {
  if (!bytes) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1 }
  return `${value.toFixed(unit ? 1 : 0)} ${units[unit]}`
}

function startEditDirectory() {
  directoryInput.value = transferStore.downloadDirectory
  editingDirectory.value = true
}

function saveDirectory() {
  const directory = directoryInput.value.trim()
  if (directory) transferStore.downloadDirectory = directory
  editingDirectory.value = false
}
</script>

<template>
  <section class="transfer-panel" :class="{ standalone: props.standalone }">
    <header class="transfer-header">
      <strong>传输</strong>
      <button title="清除已完成任务" @click="transferStore.clearCompleted(sessionId)"><Trash2 :size="14" />清除</button>
    </header>
    <div class="download-directory">
      <FolderOpen :size="14" />
      <template v-if="editingDirectory">
        <input v-model="directoryInput" @keydown.enter="saveDirectory" @keydown.esc="editingDirectory = false">
        <button @click="saveDirectory">确定</button>
      </template>
      <button v-else class="directory-value" :title="transferStore.downloadDirectory" @click="startEditDirectory">下载至 {{ transferStore.downloadDirectory }}</button>
    </div>
    <div v-if="tasks.length" class="transfer-list">
      <article v-for="task in tasks" :key="task.id" class="transfer-task">
        <component :is="task.direction === 'upload' ? Upload : Download" :size="15" class="direction" />
        <div class="task-details">
          <div class="task-name" :title="task.name">{{ task.name }}</div>
          <div class="task-meta">
            <template v-if="task.status === 'transferring'">{{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}</template>
            <template v-else-if="task.status === 'completed'">{{ formatBytes(task.totalBytes) }}</template>
            <template v-else-if="task.status === 'failed'">{{ task.error }}</template>
            <template v-else>等待中</template>
          </div>
          <div v-if="task.status === 'transferring'" class="progress"><span :style="{ width: `${task.totalBytes ? task.transferredBytes / task.totalBytes * 100 : 0}%` }" /></div>
        </div>
        <LoaderCircle v-if="task.status === 'transferring'" :size="15" class="running" />
        <Check v-else-if="task.status === 'completed'" :size="15" class="success" />
        <XCircle v-else-if="task.status === 'failed'" :size="15" class="failed" />
      </article>
    </div>
    <div v-else class="empty">暂无传输任务</div>
  </section>
</template>

<style scoped>
.transfer-panel { position: absolute; z-index: 160; top: 42px; right: 8px; width: min(380px, calc(100% - 16px)); max-height: calc(100% - 50px); overflow: auto; border: 1px solid var(--app-border); border-radius: 6px; background: var(--app-panel); box-shadow: 0 10px 24px var(--app-shadow); color: var(--app-text); }.transfer-panel.standalone { position: static; width: 100%; max-height: calc(100vh - 88px); }
.transfer-header { display: flex; align-items: center; justify-content: space-between; height: 38px; padding: 0 11px; border-bottom: 1px solid var(--app-border); font-size: 13px; }.transfer-header button, .download-directory button { display: inline-flex; align-items: center; gap: 4px; border: 0; background: transparent; color: var(--app-muted); font-size: 11px; cursor: pointer; }.transfer-header button:hover, .directory-value:hover { color: var(--app-text); }
.download-directory { display: flex; align-items: center; gap: 6px; min-height: 34px; padding: 0 11px; border-bottom: 1px solid var(--app-border); color: var(--app-muted); }.directory-value { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.download-directory input { min-width: 0; flex: 1; height: 23px; padding: 0 5px; border: 1px solid var(--app-border); border-radius: 3px; outline: none; background: var(--app-code); color: var(--app-text); font-size: 11px; }
.transfer-task { display: flex; align-items: center; gap: 9px; min-height: 55px; padding: 8px 11px; border-bottom: 1px solid var(--app-border); }.direction { flex: 0 0 auto; color: var(--app-accent); }.task-details { min-width: 0; flex: 1; }.task-name, .task-meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.task-name { font-size: 12px; }.task-meta { margin-top: 3px; color: var(--app-muted); font-size: 10px; }.progress { height: 3px; margin-top: 5px; overflow: hidden; border-radius: 2px; background: var(--app-hover); }.progress span { display: block; height: 100%; background: var(--app-accent); }.running { color: var(--app-muted); animation: spin .8s linear infinite; }.success { color: #15803d; }.failed { color: #dc2626; }.empty { padding: 28px 12px; color: var(--app-muted); text-align: center; font-size: 12px; } @keyframes spin { to { transform: rotate(360deg); } }
</style>
