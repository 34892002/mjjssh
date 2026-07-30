<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { NPopconfirm } from 'naive-ui'
import { AlertCircle, BrushCleaning, Check, FilePenLine, FolderOpen, RefreshCw, Trash2, Upload } from '@lucide/vue'
import { useLocale } from '../composables/useLocale'
import { useExternalEditorStore } from '../stores/externalEditor'

const props = defineProps<{ sessionId: string; visible: boolean }>()

const externalEditorStore = useExternalEditorStore()
const { t } = useLocale()
const sessions = computed(() => externalEditorStore.sessions.filter((session) => session.sessionId === props.sessionId))
const keepPanelVisible = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null

function statusLabel(status: string) {
  return {
    clean: t('externalEditor.clean'),
    'pending-upload': t('externalEditor.pendingUpload'),
    uploading: t('externalEditor.uploading'),
    conflict: t('externalEditor.conflict'),
    error: t('externalEditor.error'),
  }[status] ?? status
}

function refreshAll() {
  void Promise.all(sessions.value.map((session) => externalEditorStore.refresh(session.editId).catch(() => undefined)))
}

function startPolling() {
  if (pollTimer || !props.visible) return
  refreshAll()
  pollTimer = setInterval(refreshAll, 3_000)
}

function stopPolling() {
  if (!pollTimer) return
  clearInterval(pollTimer)
  pollTimer = null
}

async function upload(editId: string) {
  try {
    await externalEditorStore.upload(editId)
  } catch {
    // The per-session error is displayed by the store.
  }
}

function open(editId: string) {
  void externalEditorStore.open(editId).catch(() => undefined)
}

function reload(editId: string) {
  void externalEditorStore.reload(editId).catch(() => undefined)
}

function discard(editId: string) {
  void externalEditorStore.discard(editId).catch(() => undefined)
}

function clearAll() {
  void externalEditorStore.clearAll().catch(() => undefined)
}

function hidePanelAfterLeave() {
  if (!sessions.value.length) keepPanelVisible.value = false
}

watch(sessions, (nextSessions) => {
  if (nextSessions.length) keepPanelVisible.value = true
}, { immediate: true })

watch(() => props.visible, (visible) => {
  if (visible) startPolling()
  else stopPolling()
}, { immediate: true })
onBeforeUnmount(stopPolling)
</script>

<template>
  <section v-if="keepPanelVisible" class="external-edit-panel" :aria-label="t('externalEditor.ariaLabel')">
    <header>
      <div>
        <FilePenLine :size="15" />
        <strong>{{ t('externalEditor.title') }}</strong>
      </div>
      <div class="external-edit-header-actions">
        <NPopconfirm
          :style="{ maxWidth: '360px' }"
          :positive-text="t('externalEditor.clearAll')"
          :negative-text="t('actionDialog.cancel')"
          @positive-click="clearAll"
        >
          <template #trigger>
            <button :title="t('externalEditor.clearAllShort')" :aria-label="t('externalEditor.clearAllShort')" :disabled="externalEditorStore.isCleaning">
              <BrushCleaning :size="14" />
            </button>
          </template>
          <div class="external-edit-confirmation">
            <strong>{{ t('externalEditor.clearAllTitle') }}</strong>
            <p>{{ t('externalEditor.clearAllMessage') }}</p>
          </div>
        </NPopconfirm>
        <button :title="t('externalEditor.refreshStatus')" :aria-label="t('externalEditor.refreshStatus')" :disabled="externalEditorStore.isCleaning" @click="refreshAll"><RefreshCw :size="14" /></button>
      </div>
    </header>
    <TransitionGroup name="external-edit" tag="div" @after-leave="hidePanelAfterLeave">
      <article v-for="session in sessions" :key="session.editId" class="external-edit-session">
        <div class="external-edit-summary">
          <span class="external-edit-path" :title="session.path">{{ session.path }}</span>
          <span class="external-edit-status" :class="`is-${session.status}`">
            <Check v-if="session.status === 'clean'" :size="13" />
            <AlertCircle v-else :size="13" />
            {{ statusLabel(session.status) }}
          </span>
        </div>
        <p v-if="externalEditorStore.errors[session.editId]" class="external-edit-error">{{ externalEditorStore.errors[session.editId] }}</p>
        <div class="external-edit-actions">
          <button :disabled="externalEditorStore.loadingEditIds.has(session.editId) || session.status === 'clean'" @click="upload(session.editId)">
            <Upload :size="14" /> {{ t('externalEditor.upload') }}
          </button>
          <button :disabled="externalEditorStore.loadingEditIds.has(session.editId)" @click="open(session.editId)">
            <FolderOpen :size="14" /> {{ t('externalEditor.open') }}
          </button>
          <NPopconfirm
            :style="{ maxWidth: '360px' }"
            :positive-text="t('externalEditor.reload')"
            :negative-text="t('actionDialog.cancel')"
            @positive-click="reload(session.editId)"
          >
            <template #trigger>
              <button :disabled="externalEditorStore.loadingEditIds.has(session.editId)">
                <RefreshCw :size="14" /> {{ t('externalEditor.reload') }}
              </button>
            </template>
            <div class="external-edit-confirmation">
              <strong>{{ t('externalEditor.reloadTitle') }}</strong>
              <p>{{ t('externalEditor.reloadMessage', { path: session.path }) }}</p>
            </div>
          </NPopconfirm>
          <NPopconfirm
            :style="{ maxWidth: '360px' }"
            :positive-text="t('externalEditor.discard')"
            :negative-text="t('actionDialog.cancel')"
            @positive-click="discard(session.editId)"
          >
            <template #trigger>
              <button class="danger" :disabled="externalEditorStore.loadingEditIds.has(session.editId)">
                <Trash2 :size="14" /> {{ t('externalEditor.discard') }}
              </button>
            </template>
            <div class="external-edit-confirmation">
              <strong>{{ t('externalEditor.discardTitle') }}</strong>
              <p>{{ t('externalEditor.discardMessage', { path: session.path }) }}</p>
            </div>
          </NPopconfirm>
        </div>
      </article>
    </TransitionGroup>
  </section>
</template>

<style scoped>
.external-edit-panel { border-top: 1px solid var(--app-border); background: var(--app-surface); color: var(--app-text); }
.external-edit-panel header { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px; color: var(--app-muted); font-size: 12px; }
.external-edit-panel header > div { display: flex; align-items: center; gap: 6px; }
.external-edit-header-actions { display: flex; align-items: center; gap: 2px; }
.external-edit-panel header button { display: grid; place-items: center; border: 0; border-radius: 4px; width: 24px; height: 24px; background: transparent; color: inherit; cursor: pointer; }
.external-edit-panel header button:hover:not(:disabled) { background: var(--app-hover); color: var(--app-text); }
.external-edit-panel header button:disabled { opacity: .45; cursor: default; }
.external-edit-session { padding: 8px 10px 10px; border-top: 1px solid var(--app-border); }
.external-edit-summary { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-width: 0; }
.external-edit-path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: monospace; font-size: 12px; }
.external-edit-status { display: inline-flex; align-items: center; gap: 4px; flex: 0 0 auto; color: var(--app-muted); font-size: 11px; }
.external-edit-status.is-pending-upload { color: #d97706; }.external-edit-status.is-conflict, .external-edit-status.is-error { color: #dc2626; }.external-edit-status.is-uploading { color: var(--app-accent); }.external-edit-status.is-clean { color: #16a34a; }
.external-edit-error { margin: 7px 0 0; color: #dc2626; font-size: 11px; white-space: pre-wrap; }
.external-edit-actions { display: flex; gap: 5px; margin-top: 8px; }
.external-edit-actions button { display: inline-flex; align-items: center; justify-content: center; gap: 4px; min-height: 25px; border: 1px solid var(--app-border); border-radius: 4px; background: transparent; color: var(--app-text); padding: 3px 7px; cursor: pointer; font-size: 11px; }
.external-edit-actions button:hover:not(:disabled) { background: var(--app-hover); }.external-edit-actions button:disabled { opacity: .45; cursor: default; }.external-edit-actions .danger { color: #dc2626; }
  .external-edit-confirmation { max-width: 360px; white-space: normal; overflow-wrap: anywhere; }
  .external-edit-confirmation p { margin: 6px 0 0; line-height: 1.5; }
  .external-edit-leave-active { transition: opacity 1s ease, transform 1s ease; }
  .external-edit-leave-to { opacity: 0; transform: translateX(12px); }
</style>
