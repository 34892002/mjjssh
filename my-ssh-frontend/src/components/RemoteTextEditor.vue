<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Braces, FileSearch, RotateCcw, Save, WrapText } from '@lucide/vue'
import { NButton, NSelect, NTooltip } from 'naive-ui'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'
import { useLocale } from '../composables/useLocale'
import { useRemoteEditorStore, type RemoteEditorTab } from '../stores/remoteEditor'
import type { RemoteEditorLanguage, RemoteTextEncoding } from '../types'

const props = defineProps<{ tab: RemoteEditorTab; dark: boolean }>()
const emit = defineEmits<{ close: []; requestConfirm: [options: { title: string; message: string; confirmText: string; danger?: boolean; onConfirm: () => void }] }>()
const store = useRemoteEditorStore()
const { t } = useLocale()
const editor = ref<HTMLElement | null>(null)
let view: import('@codemirror/view').EditorView | null = null
let wrapping = true

const fileName = computed(() => props.tab.path.split('/').pop() || props.tab.path)
const statusLabel = computed(() => ({
  saved: t('remoteEditor.saved'),
  dirty: t('remoteEditor.unsavedChanges'),
  saving: t('remoteEditor.saving'),
  'remote-changed': t('remoteEditor.remoteChanged'),
  error: t('remoteEditor.saveFailed'),
}[props.tab.status]))
const encodingOptions = [
  { label: 'UTF-8', value: 'utf-8' },
  { label: 'GBK', value: 'gbk' },
  { label: 'GB18030', value: 'gb18030' },
]
const languageOptions = [
  ['plain', t('remoteEditor.plainText')], ['shell', t('remoteEditor.shell')], ['json', 'JSON'], ['yaml', 'YAML'], ['toml', 'TOML'], ['ini', t('remoteEditor.ini')], ['xml', 'XML'], ['dockerfile', 'Dockerfile'], ['sql', 'SQL'], ['terraform', 'Terraform'], ['python', 'Python'], ['go', 'Go'], ['javascript', 'JavaScript'], ['typescript', 'TypeScript'], ['java', t('remoteEditor.javaKotlin')], ['php', 'PHP'], ['ruby', 'Ruby'], ['perl', 'Perl'], ['lua', 'Lua'], ['markdown', 'Markdown'],
].map(([value, label]) => ({ value, label }))

async function languageExtension(language: RemoteEditorLanguage) {
  switch (language) {
    case 'javascript': case 'typescript': { const { javascript } = await import('@codemirror/lang-javascript'); return javascript({ typescript: language === 'typescript' }) }
    case 'json': { const { json } = await import('@codemirror/lang-json'); return json() }
    case 'yaml': { const { yaml } = await import('@codemirror/lang-yaml'); return yaml() }
    case 'xml': case 'dockerfile': { const { xml } = await import('@codemirror/lang-xml'); return xml() }
    case 'sql': { const { sql } = await import('@codemirror/lang-sql'); return sql() }
    case 'python': { const { python } = await import('@codemirror/lang-python'); return python() }
    case 'markdown': { const { markdown } = await import('@codemirror/lang-markdown'); return markdown() }
    case 'shell': {
      const [{ StreamLanguage }, { shell }] = await Promise.all([
        import('@codemirror/language'),
        import('@codemirror/legacy-modes/mode/shell'),
      ])
      return StreamLanguage.define(shell)
    }
    default: return null
  }
}

function editorTheme(EditorView: typeof import('@codemirror/view').EditorView) {
  return EditorView.theme({
    '&': { height: '100%', color: 'var(--app-text)', backgroundColor: 'var(--app-base)' },
    '.cm-scroller': { fontFamily: 'Cascadia Code, Fira Code, JetBrains Mono, Consolas, monospace' },
    '.cm-gutters': { color: 'var(--app-muted)', backgroundColor: 'var(--app-surface)', borderRight: '1px solid var(--app-border)' },
    '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--app-surface)' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--app-accent)' },
    '.cm-cursor': { borderLeftColor: 'var(--app-text)' },
  })
}

function editorHighlightStyle(dark: boolean) {
  const colors = dark
    ? {
        keyword: '#78a9e8', number: '#c99850', string: '#4ba89a', function: '#70a7cf', type: '#6ca6a0', comment: '#8791a5', invalid: '#d75f5f',
      }
    : {
        keyword: '#0057b8', number: '#9a5800', string: '#00766b', function: '#0063a6', type: '#087567', comment: '#697386', invalid: '#b42318',
      }
  return syntaxHighlighting(HighlightStyle.define([
    { tag: tags.keyword, color: colors.keyword },
    { tag: [tags.atom, tags.bool, tags.null, tags.number], color: colors.number },
    { tag: [tags.string, tags.special(tags.string)], color: colors.string },
    { tag: [tags.function(tags.variableName), tags.labelName], color: colors.function },
    { tag: [tags.typeName, tags.className, tags.namespace], color: colors.type },
    { tag: [tags.comment, tags.lineComment, tags.blockComment], color: colors.comment, fontStyle: 'italic' },
    { tag: [tags.operatorKeyword, tags.operator], color: 'var(--app-text)' },
    { tag: tags.invalid, color: colors.invalid },
  ]))
}

async function createEditor() {
  if (!editor.value) return
  const [{ basicSetup }, { EditorState }, { EditorView, keymap }, { searchKeymap, openSearchPanel }, { defaultKeymap, historyKeymap }] = await Promise.all([
    import('codemirror'), import('@codemirror/state'), import('@codemirror/view'), import('@codemirror/search'), import('@codemirror/commands'),
  ])
  const language = await languageExtension(props.tab.language)
  const saveKeymap = keymap.of([{ key: 'Mod-s', preventDefault: true, run: () => { void save(); return true } }])
  const extensions = [basicSetup, keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]), saveKeymap, editorTheme(EditorView), editorHighlightStyle(props.dark)]
  if (wrapping) extensions.push(EditorView.lineWrapping)
  if (language) extensions.push(language)
  const state = EditorState.create({
    doc: props.tab.content,
    extensions: [...extensions, EditorView.updateListener.of((update) => {
      if (update.docChanged) store.updateContent(props.tab.id, update.state.doc.toString())
    })],
  })
  view = new EditorView({ state, parent: editor.value })
  // Keep a Find button available without requiring users to remember the shortcut.
  findInEditor.value = () => openSearchPanel(view!)
}

const findInEditor = ref<() => void>(() => undefined)
async function rebuildEditor() { view?.destroy(); view = null; await createEditor() }
function setEncoding(value: RemoteTextEncoding) {
  if (value === props.tab.encoding) return
  if (props.tab.status === 'dirty') {
    emit('requestConfirm', {
      title: t('remoteEditor.changeEncodingTitle'),
      message: t('remoteEditor.changeEncodingMessage'),
      confirmText: t('remoteEditor.reloadFile'),
      danger: true,
      onConfirm: () => { store.updateTab(props.tab.id, { encoding: value }); void loadRemoteFile() },
    })
    return
  }
  store.updateTab(props.tab.id, { encoding: value })
  void loadRemoteFile()
}
async function setLanguage(value: RemoteEditorLanguage) { store.updateTab(props.tab.id, { language: value }); await rebuildEditor() }
async function toggleWrapping() { wrapping = !wrapping; await rebuildEditor() }
function reload() {
  if (props.tab.status === 'dirty') {
    emit('requestConfirm', { title: t('remoteEditor.reloadRemoteFileTitle'), message: t('remoteEditor.discardUnsavedChanges', { path: props.tab.path }), confirmText: t('remoteEditor.reload'), danger: true, onConfirm: () => { void loadRemoteFile() } })
    return
  }
  void loadRemoteFile()
}

async function loadRemoteFile() {
  try {
    const remoteFile = await invoke<{ bytes: number[]; containsNul: boolean; version: RemoteEditorTab['version'] }>('get_remote_text_file', { request: { sessionId: props.tab.sessionId, path: props.tab.path, allowLargeFile: true } })
    const decoderLabel = props.tab.encoding === 'utf-8' ? 'utf-8' : props.tab.encoding === 'gbk' ? 'gbk' : 'gb18030'
    const content = new TextDecoder(decoderLabel, { fatal: true }).decode(new Uint8Array(remoteFile.bytes))
    view?.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: content } })
    store.updateTab(props.tab.id, { content, savedContent: content, containsNul: remoteFile.containsNul, version: remoteFile.version, status: 'saved', error: null })
  } catch (error) {
    store.updateTab(props.tab.id, { status: 'error', error: String(error) })
  }
}

async function save(force = false, confirmBinaryWrite = false) {
  if (props.tab.status === 'saving') return
  if (props.tab.containsNul && !confirmBinaryWrite) {
    emit('requestConfirm', {
      title: t('remoteEditor.saveBinaryContentTitle'),
      message: t('remoteEditor.saveBinaryContentMessage'),
      confirmText: t('remoteEditor.saveAnyway'),
      danger: true,
      onConfirm: () => { void save(force, true) },
    })
    return
  }
  store.updateTab(props.tab.id, { status: 'saving', error: null })
  try {
    const result = await invoke<{ kind: 'saved'; version: RemoteEditorTab['version'] } | { kind: 'conflict'; currentVersion: RemoteEditorTab['version'] }>('save_remote_text_file', {
      request: { sessionId: props.tab.sessionId, path: props.tab.path, content: props.tab.content, encoding: props.tab.encoding, lineEnding: props.tab.lineEnding, expectedVersion: props.tab.version, force, confirmBinaryWrite },
    })
    if (result.kind === 'conflict') store.updateTab(props.tab.id, { status: 'remote-changed', version: result.currentVersion })
    else store.updateTab(props.tab.id, { version: result.version, savedContent: props.tab.content, status: 'saved' })
  } catch (error) {
    store.updateTab(props.tab.id, { status: 'error', error: String(error) })
  }
}

async function initializeEditor() {
  try {
    await createEditor()
  } catch (error) {
    store.updateTab(props.tab.id, { status: 'error', error: String(error) })
  }
}

async function recreateEditor() {
  try {
    await rebuildEditor()
  } catch (error) {
    store.updateTab(props.tab.id, { status: 'error', error: String(error) })
  }
}

onMounted(() => { void initializeEditor() })
onBeforeUnmount(() => { view?.destroy(); view = null })
watch(() => props.tab.id, () => { void recreateEditor() })
watch(() => props.dark, () => { void recreateEditor() })
</script>

<template>
  <section class="remote-editor">
    <header class="editor-toolbar">
      <div class="file-details">
        <strong>{{ fileName }}</strong>
        <span :title="tab.path">{{ tab.path }}</span>
      </div>
      <span class="editor-status" :class="`is-${tab.status}`">{{ statusLabel }}</span>
      <n-select class="editor-select" size="small" :value="tab.encoding" :options="encodingOptions" @update:value="setEncoding" />
      <n-select class="editor-select language-select" size="small" :value="tab.language" :options="languageOptions" @update:value="setLanguage" />
      <n-tooltip><template #trigger><n-button quaternary size="small" :aria-label="t('remoteEditor.save')" :disabled="tab.status === 'saving'" @click="() => { void save() }"><template #icon><Save :size="16" /></template></n-button></template>{{ t('remoteEditor.save') }}</n-tooltip>
      <n-tooltip v-if="tab.status === 'remote-changed'"><template #trigger><n-button quaternary type="warning" size="small" :aria-label="t('remoteEditor.overwriteRemoteChangesTitle')" @click="emit('requestConfirm', { title: t('remoteEditor.overwriteRemoteChangesTitle'), message: t('remoteEditor.overwriteRemoteChangesMessage', { path: tab.path }), confirmText: t('remoteEditor.overwrite'), danger: true, onConfirm: () => { void save(true) } })"><template #icon><Save :size="16" /></template></n-button></template>{{ t('remoteEditor.overwriteRemoteChangesTitle') }}</n-tooltip>
      <n-tooltip><template #trigger><n-button quaternary size="small" :aria-label="t('remoteEditor.revertToSaved')" @click="reload"><template #icon><RotateCcw :size="16" /></template></n-button></template>{{ t('remoteEditor.revertToSaved') }}</n-tooltip>
      <n-tooltip><template #trigger><n-button quaternary size="small" :aria-label="t('remoteEditor.findAndReplace')" @click="findInEditor()"><template #icon><FileSearch :size="16" /></template></n-button></template>{{ t('remoteEditor.findAndReplace') }}</n-tooltip>
      <n-tooltip><template #trigger><n-button quaternary size="small" :aria-label="t('remoteEditor.toggleLineWrapping')" @click="toggleWrapping"><template #icon><WrapText :size="16" /></template></n-button></template>{{ t('remoteEditor.toggleLineWrapping') }}</n-tooltip>
      <n-tooltip><template #trigger><n-button quaternary size="small" :aria-label="t('remoteEditor.closeEditor')" @click="emit('close')"><template #icon><Braces :size="16" /></template></n-button></template>{{ t('remoteEditor.closeEditor') }}</n-tooltip>
    </header>
    <p v-if="tab.error" class="editor-error">{{ tab.error }}</p>
    <div ref="editor" class="editor-host" />
  </section>
</template>

<style scoped>
.remote-editor { display: flex; min-width: 0; min-height: 0; height: 100%; flex: 1; flex-direction: column; background: var(--app-base); }
.editor-toolbar { display: flex; min-height: 46px; align-items: center; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--app-border); background: var(--app-surface); }
.file-details { display: flex; min-width: 0; margin-right: auto; flex-direction: column; line-height: 1.25; } .file-details strong { color: var(--app-text); } .file-details span { overflow: hidden; max-width: 360px; color: var(--app-muted); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
.editor-status { font-size: 12px; color: var(--app-muted); white-space: nowrap; } .editor-status.is-dirty, .editor-status.is-error, .editor-status.is-remote-changed { color: #d48806; } .editor-status.is-saving { color: var(--app-accent); }
.editor-select { width: 106px; } .language-select { width: 136px; } .editor-host { min-height: 0; flex: 1; overflow: hidden; } .editor-error { margin: 0; padding: 6px 12px; color: #c73e1d; background: color-mix(in srgb, #c73e1d 10%, var(--app-base)); font-size: 12px; }
</style>
