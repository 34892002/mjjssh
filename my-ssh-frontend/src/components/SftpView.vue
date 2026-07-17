<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { Archive, ChevronLeft, Copy, Download, File, Folder, FolderPlus, FolderTree, Pencil, RefreshCw, Shield, Trash2, Upload, X } from '@lucide/vue'
import { useTransferStore } from '../stores/transfer'

type FileInfo = { name: string; is_dir: boolean; size: number; modified: string; mode: number }
type SortKey = 'name' | 'modified' | 'size'

const props = defineProps<{ sessionId: string; dark: boolean }>()
const transferStore = useTransferStore()
const emit = defineEmits<{
  editPermissions: [file: FileInfo, path: string]
  requestInput: [options: { title: string; initialValue?: string; placeholder?: string; onConfirm: (value: string) => void }]
  requestConfirm: [options: { title: string; message: string; confirmText: string; danger?: boolean; onConfirm: () => void }]
  close: []
}>()
const currentPath = ref('/')
const files = ref<FileInfo[]>([])
const loading = ref(false)
const uploading = ref(false)
const error = ref('')
const isDragOver = ref(false)
const selectedFile = ref<FileInfo | null>(null)
const sortKey = ref<SortKey>('name')
const sortAscending = ref(true)
const menu = ref<{ file: FileInfo | null; x: number; y: number } | null>(null)
const contextMenu = ref<HTMLElement | null>(null)
let unlistenDragDrop: UnlistenFn | null = null

const pathSegments = computed(() => currentPath.value.split('/').filter(Boolean))
const sortedFiles = computed(() => [...files.value].sort((a, b) => {
  if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
  const left = sortKey.value === 'size' ? a.size : a[sortKey.value]
  const right = sortKey.value === 'size' ? b.size : b[sortKey.value]
  const comparison = typeof left === 'number' ? left - (right as number) : String(left).localeCompare(String(right))
  return sortAscending.value ? comparison : -comparison
}))

function joinPath(base: string, name: string): string { return `${base === '/' ? '' : base}/${name}` }
function setSort(key: SortKey) {
  if (sortKey.value === key) sortAscending.value = !sortAscending.value
  else { sortKey.value = key; sortAscending.value = true }
}

async function listFiles(path = currentPath.value) {
  loading.value = true
  error.value = ''
  menu.value = null
  try {
    files.value = await invoke<FileInfo[]>('sftp_list_files', { sessionId: props.sessionId, path })
    currentPath.value = path
    selectedFile.value = null
  } catch (e) { error.value = String(e) } finally { loading.value = false }
}
function goUp() {
  if (currentPath.value === '/') return
  const segments = [...pathSegments.value]
  segments.pop()
  void listFiles(`/${segments.join('/')}` || '/')
}
function openDirectory(file: FileInfo) { if (file.is_dir) void listFiles(joinPath(currentPath.value, file.name)) }

function enqueueUpload(paths: string[], overwrite = false) {
  transferStore.enqueueUpload(props.sessionId, paths, currentPath.value, overwrite)
  void listFiles()
}

async function uploadFiles(paths: string[]) {
  if (!paths.length) return
  uploading.value = true; error.value = ''
  const conflicts = paths
    .map((path) => path.split(/[\\/]/).pop() || path)
    .filter((name) => files.value.some((file) => file.name === name))

  if (conflicts.length) {
    const preview = conflicts.slice(0, 3).map((name) => `“${name}”`).join('、')
    const remaining = conflicts.length - 3
    emit('requestConfirm', {
      title: '覆盖远程文件',
      message: `${preview}${remaining > 0 ? ` 等 ${conflicts.length} 个文件` : ''}已存在于当前目录，继续上传将覆盖原文件。`,
      confirmText: '覆盖并上传',
      danger: true,
      onConfirm: () => enqueueUpload(paths, true),
    })
  } else {
    enqueueUpload(paths)
  }
  uploading.value = false
}
function handleDrop(event: DragEvent) {
  event.preventDefault(); isDragOver.value = false
  const paths = Array.from(event.dataTransfer?.files ?? []).map((file) => (file as File & { path?: string }).path).filter((path): path is string => Boolean(path))
  void uploadFiles(paths)
}
function positionMenu() {
  void nextTick(() => {
    if (!menu.value || !contextMenu.value) return
    const { width, height } = contextMenu.value.getBoundingClientRect()
    menu.value = {
      ...menu.value,
      x: Math.max(8, Math.min(menu.value.x, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(menu.value.y, window.innerHeight - height - 8)),
    }
  })
}
function showMenu(event: MouseEvent, file: FileInfo) {
  event.preventDefault(); selectedFile.value = file
  menu.value = { file, x: event.clientX, y: event.clientY }
  positionMenu()
}
function showBackgroundMenu(event: MouseEvent) {
  event.preventDefault(); selectedFile.value = null
  menu.value = { file: null, x: event.clientX, y: event.clientY }
  positionMenu()
}
function closeMenu() { menu.value = null }
function handleDocumentPointerDown() { closeMenu() }
function createDirectory() {
  emit('requestInput', {
    title: '新建文件夹',
    placeholder: '输入文件夹名称',
    onConfirm: async (name) => {
      try { await invoke('sftp_create_directory', { sessionId: props.sessionId, path: joinPath(currentPath.value, name) }); await listFiles() } catch (e) { error.value = `新建失败: ${String(e)}` }
    },
  })
}
function renameFile(file: FileInfo) {
  emit('requestInput', {
    title: '重命名',
    initialValue: file.name,
    placeholder: '输入新名称',
    onConfirm: async (name) => {
      if (name === file.name) return
      try {
        await invoke('sftp_rename', { sessionId: props.sessionId, oldPath: joinPath(currentPath.value, file.name), newPath: joinPath(currentPath.value, name) })
        await listFiles()
      } catch (e) { error.value = `重命名失败: ${String(e)}` }
    },
  })
}
function deleteFile(file: FileInfo) {
  const kind = file.is_dir ? '文件夹' : '文件'
  emit('requestConfirm', {
    title: `删除${kind}`,
    message: `确定删除${kind}“${file.name}”吗？`,
    confirmText: '删除',
    danger: true,
    onConfirm: async () => {
      try { await invoke('sftp_delete', { sessionId: props.sessionId, path: joinPath(currentPath.value, file.name), isDir: file.is_dir }); await listFiles() } catch (e) { error.value = `删除失败: ${String(e)}` }
    },
  })
}
async function copyPath(file: FileInfo) { await navigator.clipboard.writeText(joinPath(currentPath.value, file.name)); closeMenu() }
function isTarGz(file: FileInfo) { return /\.(tar\.gz|tgz)$/i.test(file.name) }
function openPermissions(file: FileInfo) { emit('editPermissions', file, joinPath(currentPath.value, file.name)); closeMenu() }
async function downloadFile(file: FileInfo) {
  if (file.is_dir) return
  closeMenu()
  try {
    await transferStore.initialize()
    const localFileExists = await invoke<boolean>('sftp_local_file_exists', {
      localDirectory: transferStore.downloadDirectory,
      fileName: file.name,
    })
    const enqueueDownload = (overwrite = false) => transferStore.enqueueDownload(
      props.sessionId,
      joinPath(currentPath.value, file.name),
      file.name,
      file.size,
      overwrite,
    )
    if (localFileExists) {
      emit('requestConfirm', {
        title: '覆盖本地文件',
        message: `“${file.name}”已存在于下载目录，继续下载将覆盖原文件。`,
        confirmText: '覆盖并下载',
        danger: true,
        onConfirm: () => enqueueDownload(true),
      })
    } else {
      enqueueDownload()
    }
  } catch (e) {
    error.value = `下载失败: ${String(e)}`
  }
}
async function compressFile(file: FileInfo) {
  try { await invoke('sftp_compress_tar_gz', { sessionId: props.sessionId, path: joinPath(currentPath.value, file.name) }); await listFiles() } catch (e) { error.value = `压缩失败: ${String(e)}` }
  closeMenu()
}
async function extractFile(file: FileInfo) {
  try { await invoke('sftp_extract_tar_gz', { sessionId: props.sessionId, path: joinPath(currentPath.value, file.name) }); await listFiles() } catch (e) { error.value = `解压失败: ${String(e)}` }
  closeMenu()
}
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`; if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`; if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`; return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}
async function openHomeDirectory() {
  try {
    const homeDirectory = await invoke<string>('sftp_get_home_directory', { sessionId: props.sessionId })
    await listFiles(homeDirectory)
  } catch {
    await listFiles('/')
  }
}

watch(() => props.sessionId, () => { currentPath.value = '/'; files.value = []; void openHomeDirectory() })
onMounted(async () => {
  await transferStore.initialize()
  document.addEventListener('pointerdown', handleDocumentPointerDown)
  unlistenDragDrop = await getCurrentWindow().onDragDropEvent((event) => { if (event.payload.type === 'over') isDragOver.value = true; if (event.payload.type === 'leave') isDragOver.value = false; if (event.payload.type === 'drop') { isDragOver.value = false; void uploadFiles(event.payload.paths) } })
  void openHomeDirectory()
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', handleDocumentPointerDown)
  unlistenDragDrop?.()
})
</script>

<template>
  <section class="sftp-content-inner" @click="closeMenu" @dragover.prevent="isDragOver = true" @dragleave="isDragOver = false" @drop="handleDrop">
    <div class="sftp-toolbar">
      <div class="sftp-nav">
        <button class="sftp-btn" title="上一级目录" :disabled="currentPath === '/'" @click.stop="goUp"><ChevronLeft :size="16" /></button>
        <button class="sftp-btn" title="刷新" :disabled="loading" @click.stop="listFiles()"><RefreshCw :size="15" :class="{ spinning: loading }" /></button>
        <button class="sftp-btn" title="新建文件夹" @click.stop="createDirectory"><FolderPlus :size="16" /></button>
        <div class="sftp-path" :title="currentPath">{{ currentPath }}</div>
        <span class="item-count">{{ files.length }} 项</span>
        <button class="sftp-btn sftp-close" title="关闭 SFTP" @click.stop="emit('close')"><X :size="15" /></button>
      </div>
    </div>


    <div class="sftp-file-list" @contextmenu="showBackgroundMenu">
      <div v-if="loading" class="sftp-status">加载中...</div>
      <div v-else-if="error" class="sftp-error">{{ error }}</div>
      <table v-else class="sftp-table">
        <thead><tr><th @click="setSort('name')">名称 <span v-if="sortKey === 'name'">{{ sortAscending ? '↑' : '↓' }}</span></th><th @click="setSort('modified')">修改时间 <span v-if="sortKey === 'modified'">{{ sortAscending ? '↑' : '↓' }}</span></th><th @click="setSort('size')">大小 <span v-if="sortKey === 'size'">{{ sortAscending ? '↑' : '↓' }}</span></th></tr></thead>
        <tbody>
          <tr v-for="file in sortedFiles" :key="file.name" :class="{ selected: selectedFile?.name === file.name }" @click.stop="selectedFile = file" @dblclick="openDirectory(file)" @contextmenu.stop="showMenu($event, file)">
            <td class="sftp-name" :title="file.name"><Folder v-if="file.is_dir" class="folder-icon" :size="16" /><File v-else class="file-icon" :size="16" /><span>{{ file.name }}</span></td>
            <td class="sftp-muted">{{ file.modified }}</td><td class="sftp-muted">{{ file.is_dir ? '--' : formatSize(file.size) }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <Teleport to="body">
      <div v-if="menu" ref="contextMenu" class="context-menu" :class="{ 'theme-dark': dark }" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @pointerdown.stop @click.stop>
        <template v-if="menu.file?.is_dir">
          <button @click="openDirectory(menu.file!); closeMenu()"><FolderTree :size="15" />打开</button>
          <button @click="copyPath(menu.file!)"><Copy :size="15" />复制文件夹路径</button>
          <hr>
          <button @click="openPermissions(menu.file!)"><Shield :size="15" />编辑权限</button>
          <button @click="compressFile(menu.file!)"><Archive :size="15" />压缩为 tar.gz</button>
          <button @click="renameFile(menu.file!); closeMenu()"><Pencil :size="15" />重命名</button>
          <button class="danger" @click="deleteFile(menu.file!); closeMenu()"><Trash2 :size="15" />删除空文件夹</button>
        </template>
        <template v-else-if="menu.file">
          <button @click="downloadFile(menu.file)"><Download :size="15" />下载</button>
          <button @click="copyPath(menu.file)"><Copy :size="15" />复制文件路径</button>
          <hr>
          <button @click="openPermissions(menu.file)"><Shield :size="15" />编辑权限</button>
          <button v-if="isTarGz(menu.file)" @click="extractFile(menu.file)"><Archive :size="15" />解压到当前目录</button>
          <button v-else @click="compressFile(menu.file)"><Archive :size="15" />压缩为 tar.gz</button>
          <button @click="renameFile(menu.file); closeMenu()"><Pencil :size="15" />重命名</button>
          <button class="danger" @click="deleteFile(menu.file); closeMenu()"><Trash2 :size="15" />删除文件</button>
        </template>
        <template v-else>
          <button @click="listFiles(); closeMenu()"><RefreshCw :size="15" />刷新</button>
          <button @click="createDirectory(); closeMenu()"><FolderPlus :size="15" />新建文件夹</button>
        </template>
      </div>
    </Teleport>

    <div v-if="isDragOver || uploading" class="drop-overlay"><Upload :size="22" />{{ uploading ? '正在上传...' : '松开以上传到当前目录' }}</div>
  </section>
</template>

<style scoped>
.sftp-content-inner { position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--app-panel); color: var(--app-text); font-size: 13px; }
.sftp-toolbar { display: flex; align-items: center; padding: 7px 10px; background: var(--app-surface); border-bottom: 1px solid var(--app-border); }.sftp-nav { display: flex; align-items: center; gap: 3px; min-width: 0; width: 100%; }
.sftp-btn { display: grid; place-items: center; flex: 0 0 27px; width: 27px; height: 27px; border: 0; border-radius: 4px; background: transparent; color: var(--app-muted); cursor: pointer; }.sftp-btn:hover:not(:disabled) { background: var(--app-hover); color: var(--app-text); }.sftp-btn:disabled { opacity: .35; cursor: default; }
.sftp-path { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 4px 7px; color: var(--app-text); font-family: monospace; }.item-count { flex: 0 0 auto; color: var(--app-muted); font-size: 11px; padding-left: 5px; }.sftp-close { margin-left: 5px; }.sftp-close:hover { background: #f38ba8 !important; color: #fff !important; }
.sftp-file-list { flex: 1; min-height: 0; overflow: auto; }.sftp-status, .sftp-error { padding: 40px 12px; text-align: center; color: var(--app-muted); }.sftp-error { color: #dc2626; white-space: pre-wrap; }
.sftp-table { width: 100%; min-width: 0; border-collapse: collapse; table-layout: fixed; }.sftp-table th, .sftp-table td { height: 36px; padding: 0 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }.sftp-table thead { border-bottom: 1px solid #313244; }.sftp-table tbody tr { border-bottom: 1px solid rgba(49, 50, 68, .65); }.sftp-table th { color: #9399b2; background: #181825; font-size: 12px; font-weight: 500; cursor: pointer; position: sticky; top: 0; }.sftp-table th:first-child, .sftp-table td:first-child { width: auto; }.sftp-table th:nth-child(2), .sftp-table td:nth-child(2) { width: 126px; }.sftp-table th:nth-child(3), .sftp-table td:nth-child(3) { width: 54px; }
.sftp-table tbody tr { cursor: pointer; }.sftp-table tbody tr > td { transition: background-color .12s ease, color .12s ease; }.sftp-table tbody tr:hover > td { background: #313244; color: #cdd6f4; }.sftp-table tbody tr:hover > td:first-child { border-left: 3px solid #89b4fa; padding-left: 7px; }.sftp-table tbody tr:hover .folder-icon { color: #b4d0fb; }.sftp-table tbody tr:hover .file-icon { color: #cdd6f4; }.sftp-table tbody tr.selected > td { background: #3b6496; color: #f5f7ff; }.sftp-table tbody tr.selected > td:first-child { border-left: 3px solid #89b4fa; padding-left: 7px; }.sftp-name { display: flex; align-items: center; gap: 9px; min-width: 0; }.sftp-name span { overflow: hidden; text-overflow: ellipsis; }.folder-icon { flex: 0 0 auto; color: #89b4fa; }.file-icon { flex: 0 0 auto; color: #a6adc8; }.sftp-muted { color: #a6adc8; font-size: 12px; }
.context-menu { position: fixed; z-index: 200; min-width: 210px; padding: 5px; border: 1px solid #45475a; border-radius: 6px; background: #1e1e2e; box-shadow: 0 10px 24px rgba(0, 0, 0, .42); }.context-menu button { display: flex; align-items: center; gap: 9px; width: 100%; height: 31px; padding: 0 8px; border: 0; border-radius: 3px; background: transparent; color: #cdd6f4; font-size: 13px; text-align: left; cursor: pointer; }.context-menu button:hover { background: #313244; }.context-menu .danger { color: #f38ba8; }.context-menu hr { border: 0; border-top: 1px solid #313244; margin: 5px 0; }

.drop-overlay { position: absolute; inset: 0; z-index: 150; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; background: rgba(30, 30, 46, .94); outline: 2px dashed #89b4fa; outline-offset: -12px; color: #cdd6f4; font-size: 15px; }.spinning { animation: spin .8s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
.sftp-table thead { border-bottom-color: var(--app-border); }
.sftp-table tbody tr { border-bottom-color: var(--app-border); }
.sftp-table th { background: var(--app-surface); color: var(--app-muted); }
.sftp-table tbody tr:hover > td { background: var(--app-hover); color: var(--app-text); }
.sftp-table tbody tr:hover > td:first-child, .sftp-table tbody tr.selected > td:first-child { border-left-color: var(--app-accent); }
.sftp-table tbody tr.selected > td { background: var(--app-selection); color: var(--app-text); }
.folder-icon { color: var(--app-accent); }
.file-icon, .sftp-muted { color: var(--app-muted); }
.context-menu { border-color: var(--app-border); background: var(--app-panel); box-shadow: 0 10px 24px var(--app-shadow); }
.context-menu button { color: var(--app-text); }
.context-menu button:hover { background: var(--app-hover); }
.context-menu hr { border-top-color: var(--app-border); }
.drop-overlay { background: color-mix(in srgb, var(--app-panel) 94%, transparent); outline-color: var(--app-accent); color: var(--app-text); }
.context-menu {
  --app-panel: #ffffff;
  --app-border: #dbe3ef;
  --app-shadow: rgba(15, 23, 42, .16);
  --app-text: #1e293b;
  --app-hover: #e8eef7;
}
.context-menu.theme-dark {
  --app-panel: #151a25;
  --app-border: #344057;
  --app-shadow: rgba(0, 0, 0, .32);
  --app-text: #cdd6f4;
  --app-hover: #252e3e;
}
</style>
