<script setup lang="ts">
import { computed, nextTick, ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { ChevronDown, ChevronUp, History, PanelBottomOpen, Regex, X, Zap } from '@lucide/vue'
import { invoke } from '@tauri-apps/api/core'
import { NPopconfirm } from 'naive-ui'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import '@xterm/xterm/css/xterm.css'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSessionStore } from '../stores/session'
import type { TerminalSettings } from '../types'

const props = defineProps<{
  sessionId: string
  kind: 'ssh' | 'local'
  dark: boolean
  reconnectVersion?: number
  settings: TerminalSettings
}>()

const emit = defineEmits<{
  disconnected: [reason: string]
  reconnect: []
}>()

const containerRef = ref<HTMLDivElement | null>(null)
const searchInputRef = ref<HTMLInputElement | null>(null)
const searchVisible = ref(false)
const searchQuery = ref('')
const searchCaseSensitive = ref(false)
const searchRegex = ref(false)
const sessionStore = useSessionStore()
const commandInputRef = ref<HTMLInputElement | null>(null)
const commandDraft = ref('')
const commandBarVisible = ref(true)
const commandHistory = ref<string[]>([])
const historyVisible = ref(false)
const historyIndex = ref(-1)
const historyDraft = ref('')
const hasSelection = ref(false)
const isSsh = computed(() => props.kind === 'ssh')


let terminal: Terminal
let fitAddon: FitAddon
let searchAddon: SearchAddon
let resizeObserver: ResizeObserver | null = null
let unlistenData: UnlistenFn | null = null
let unlistenDisconnected: UnlistenFn | null = null
let resizeRegistrationTimer: ReturnType<typeof setTimeout> | null = null
let fitFrame: number | null = null
const decoder = new TextDecoder()
let terminalUnavailable = false
let terminalCommandBuffer = ''
let terminalCommandCursor = 0
let terminalEscapeSequence = ''
let leftControlPending = false
let altPending = false

function terminalTheme() {
  return props.dark
    ? {
        background: '#111722', foreground: '#c9d1df', cursor: '#d8deeb', cursorAccent: '#111722', selectionBackground: '#30394b',
        black: '#111722', red: '#f38ba8', green: '#a6e3a1', yellow: '#f9e2af', blue: '#89b4fa', magenta: '#f5c2e7', cyan: '#94e2d5', white: '#bac2de',
        brightBlack: '#585b70', brightRed: '#f38ba8', brightGreen: '#a6e3a1', brightYellow: '#f9e2af', brightBlue: '#89b4fa', brightMagenta: '#f5c2e7', brightCyan: '#94e2d5', brightWhite: '#a6adc8',
      }
    : {
        background: '#f8fafc', foreground: '#172033', cursor: '#2563eb', cursorAccent: '#f8fafc', selectionBackground: '#bfdbfe',
        black: '#172033', red: '#dc2626', green: '#15803d', yellow: '#a16207', blue: '#2563eb', magenta: '#a21caf', cyan: '#0f766e', white: '#e2e8f0',
        brightBlack: '#64748b', brightRed: '#ef4444', brightGreen: '#22c55e', brightYellow: '#ca8a04', brightBlue: '#3b82f6', brightMagenta: '#c026d3', brightCyan: '#0891b2', brightWhite: '#ffffff',
      }
}

function searchOptions() {
  return {
    caseSensitive: searchCaseSensitive.value,
    regex: searchRegex.value,
  }
}

function runSearch(forward: boolean) {
  if (!searchQuery.value || !searchAddon) return false
  return forward
    ? searchAddon.findNext(searchQuery.value, searchOptions())
    : searchAddon.findPrevious(searchQuery.value, searchOptions())
}

function openSearch() {
  searchVisible.value = true
  void nextTick(() => searchInputRef.value?.select())
}

function closeSearch() {
  searchVisible.value = false
  searchAddon?.clearDecorations()
  terminal?.focus()
}

function handleSearchKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    closeSearch()
  }
  if (event.key === 'Enter') {
    event.preventDefault()
    runSearch(!event.shiftKey)
  }
}

function markTerminalUnavailable(message: string) {
  if (terminalUnavailable) return
  terminalUnavailable = true
  terminal.options.disableStdin = true
  terminal.write(`\r\n\x1b[31m[Terminal unavailable: ${message}]\x1b[0m\r\n`)
}

async function refreshCommandHistory() {
  try {
    commandHistory.value = await invoke<string[]>('list_command_history')
  } catch {
    commandHistory.value = []
  }
}

async function recordCommand(command: string) {
  const trimmed = command.trim()
  if (!trimmed) return
  try {
    commandHistory.value = await invoke<string[]>('record_command_history', { command: trimmed })
  } catch {
    // The command has already been sent to the terminal; history persistence is best effort.
  }
}

function resetTerminalCommandBuffer() {
  terminalCommandBuffer = ''
  terminalCommandCursor = 0
  terminalEscapeSequence = ''
}

function applyTerminalEscapeSequence(sequence: string) {
  if (sequence === '\x1b[D') {
    terminalCommandCursor = Math.max(0, terminalCommandCursor - 1)
  } else if (sequence === '\x1b[C') {
    terminalCommandCursor = Math.min(terminalCommandBuffer.length, terminalCommandCursor + 1)
  } else if (sequence === '\x1b[H' || sequence === '\x1bOH') {
    terminalCommandCursor = 0
  } else if (sequence === '\x1b[F' || sequence === '\x1bOF') {
    terminalCommandCursor = terminalCommandBuffer.length
  } else if (sequence === '\x1b[3~') {
    terminalCommandBuffer = `${terminalCommandBuffer.slice(0, terminalCommandCursor)}${terminalCommandBuffer.slice(terminalCommandCursor + 1)}`
  }
}

function trackTerminalInput(data: string) {
  for (const character of data) {
    if (terminalEscapeSequence) {
      terminalEscapeSequence += character
      const isCsiOrSs3Prefix = terminalEscapeSequence.length === 2 && (character === '[' || character === 'O')
      if ((!isCsiOrSs3Prefix && character >= '@' && character <= '~') || terminalEscapeSequence.length > 12) {
        applyTerminalEscapeSequence(terminalEscapeSequence)
        terminalEscapeSequence = ''
      }
      continue
    }
    if (character === '\x1b') {
      terminalEscapeSequence = character
    } else if (character === '\r' || character === '\n') {
      void recordCommand(terminalCommandBuffer)
      resetTerminalCommandBuffer()
    } else if (character === '\u0003') {
      resetTerminalCommandBuffer()
    } else if (character === '\u0001') {
      terminalCommandCursor = 0
    } else if (character === '\u0005') {
      terminalCommandCursor = terminalCommandBuffer.length
    } else if (character === '\b' || character === '\x7f') {
      if (terminalCommandCursor > 0) {
        terminalCommandBuffer = `${terminalCommandBuffer.slice(0, terminalCommandCursor - 1)}${terminalCommandBuffer.slice(terminalCommandCursor)}`
        terminalCommandCursor -= 1
      }
    } else if (character >= ' ') {
      terminalCommandBuffer = `${terminalCommandBuffer.slice(0, terminalCommandCursor)}${character}${terminalCommandBuffer.slice(terminalCommandCursor)}`
      terminalCommandCursor += character.length
    }
  }
}

async function writeTerminalData(data: string, track = true) {
  if (terminalUnavailable) return false
  if (!await sessionStore.writeData(props.sessionId, data)) {
    markTerminalUnavailable('input could not be delivered; reconnect to continue.')
    return false
  }
  if (track) trackTerminalInput(data)
  return true
}

function updateSelectionState() {
  hasSelection.value = Boolean(terminal?.getSelection())
}

function openCommandBar(focusInput = true) {
  commandBarVisible.value = true
  if (focusInput) void nextTick(() => commandInputRef.value?.focus())
}

function closeCommandBar() {
  historyVisible.value = false
  leftControlPending = false
  altPending = false
  commandBarVisible.value = false
}

function fillCommandInputFromSelection() {
  const selection = terminal?.getSelection()
  if (!selection) return
  commandDraft.value = selection
  openCommandBar()
}

function toggleHistory() {
  openCommandBar(false)
  historyVisible.value = !historyVisible.value
  historyIndex.value = -1
  historyDraft.value = commandDraft.value
  if (historyVisible.value) {
    void refreshCommandHistory()
    void nextTick(() => commandInputRef.value?.focus())
  }
}

function selectHistory(offset: number) {
  if (!commandHistory.value.length) return
  const nextIndex = Math.max(-1, Math.min(commandHistory.value.length - 1, historyIndex.value + offset))
  historyIndex.value = nextIndex
  commandDraft.value = nextIndex === -1 ? historyDraft.value : commandHistory.value[nextIndex]
}

function useHistoryEntry(entry: string) {
  commandDraft.value = entry
  historyVisible.value = false
  historyIndex.value = -1
  openCommandBar()
}

async function clearHistory() {
  await invoke('clear_command_history')
  commandHistory.value = []
  historyIndex.value = -1
}

async function submitCommand() {
  const command = commandDraft.value.trim()
  if (!command) return
  if (await writeTerminalData(`${command}\r`)) {
    commandDraft.value = ''
    historyVisible.value = false
    historyIndex.value = -1
  }
}

function handleCommandInputKeydown(event: KeyboardEvent) {
  if (event.key === 'ArrowUp') {
    event.preventDefault()
    selectHistory(1)
  } else if (event.key === 'ArrowDown') {
    event.preventDefault()
    selectHistory(-1)
  } else if (event.key === 'Escape') {
    event.preventDefault()
    historyVisible.value = false
  }
}

onMounted(async () => {
  if (!containerRef.value) return

  terminal = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    scrollback: props.settings.scrollbackLines,
    fontSize: props.settings.fontSize,
    fontFamily: props.settings.fontFamily,
    theme: terminalTheme(),
  })

  fitAddon = new FitAddon()
  searchAddon = new SearchAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(searchAddon)
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'

  terminal.open(containerRef.value)
  containerRef.value.addEventListener('mouseup', updateSelectionState)
  terminal.onSelectionChange(updateSelectionState)

  // 右键复制/粘贴，屏蔽浏览器菜单
  containerRef.value.addEventListener('contextmenu', (e) => {
    e.preventDefault()
    const selection = terminal.getSelection()
    if (selection) {
      // 有选中文本 → 复制
      import('@tauri-apps/plugin-clipboard-manager').then(({ writeText }) => {
        writeText(selection)
          .then(() => {
            sessionStore.rememberTerminalSelection(props.sessionId, selection)
            terminal.clearSelection()
          })
          .catch(() => {})
      })
    } else {
      // 无选中文本 → 发送剪贴板内容给远端 shell，不能只写入本地终端。
      import('@tauri-apps/plugin-clipboard-manager').then(({ readText }) => {
        readText().then((text) => {
          if (text) void writeTerminalData(text)
        }).catch(() => {})
      })
    }
  })

  try {
    const webgl = new WebglAddon()
    terminal.loadAddon(webgl)
  } catch (e) {
    console.warn('WebGL addon failed, falling back to canvas:', e)
  }

  terminal.attachCustomKeyEventHandler((event) => {
    if (!commandBarVisible.value) return true
    if (event.type === 'keydown' && event.key === 'Control' && event.location === KeyboardEvent.DOM_KEY_LOCATION_LEFT) {
      leftControlPending = true
      return true
    }
    if (event.type === 'keyup' && event.key === 'Control' && event.location === KeyboardEvent.DOM_KEY_LOCATION_LEFT) {
      if (leftControlPending && hasSelection.value) fillCommandInputFromSelection()
      leftControlPending = false
      return true
    }
    if (event.type === 'keydown' && event.key === 'Alt') {
      altPending = true
      return true
    }
    if (event.type === 'keyup' && event.key === 'Alt') {
      if (altPending) toggleHistory()
      altPending = false
      return true
    }
    if (event.type === 'keydown' && (leftControlPending || altPending)) {
      leftControlPending = false
      altPending = false
    }
    if (event.type === 'keydown' && event.key === 'Backspace') {
      void writeTerminalData(props.settings.backspaceSends === 'bs' ? '\b' : '\x7f')
      return false
    }
    if (event.type === 'keydown' && event.altKey && !event.ctrlKey && event.key.length === 1) {
      const data = props.settings.altSendsEscape
        ? `\x1b${event.key}`
        : String.fromCharCode(event.key.charCodeAt(0) | 0x80)
      void writeTerminalData(data)
      return false
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
      if (event.type === 'keydown') openSearch()
      return false
    }
    if (event.key === 'Escape' && searchVisible.value && event.type === 'keydown') {
      closeSearch()
      return false
    }
    return true
  })

  terminal.onData((data) => {
    void writeTerminalData(data)
  })

  void refreshCommandHistory()

  // 延迟注册 onResize，避免 xterm open 时立即触发导致 Session not found
  resizeRegistrationTimer = setTimeout(() => {
    terminal.onResize(({ cols, rows }) => {
      void sessionStore.resize(props.sessionId, cols, rows)
    })
  }, 500)

  // Wait for the browser to apply layout before calculating xterm's grid.
  resizeObserver = new ResizeObserver(() => {
    scheduleFit()
  })
  resizeObserver.observe(containerRef.value)


  const eventPrefix = props.kind === 'local' ? 'local-terminal' : 'ssh'
  unlistenData = await listen<number[]>(`${eventPrefix}-data:${props.sessionId}`, (event) => {
    const bytes = new Uint8Array(event.payload)
    const text = decoder.decode(bytes, { stream: true })
    if (text) terminal.write(text)
  })
  unlistenDisconnected = await listen<string>(`${eventPrefix}-${props.kind === 'local' ? 'closed' : 'disconnected'}:${props.sessionId}`, (event) => {
    markTerminalUnavailable(props.kind === 'local' ? event.payload : `connection disconnected: ${event.payload}`)
    if (props.kind === 'ssh') emit('disconnected', event.payload)
  })
  sessionStore.notifyTerminalReady(props.sessionId)
})

watch(() => props.dark, () => {
  if (terminal) terminal.options.theme = terminalTheme()
})

watch(() => props.reconnectVersion, (version, previousVersion) => {
  if (props.kind !== 'ssh' || version === undefined || version === previousVersion || !terminalUnavailable) return
  terminalUnavailable = false
  terminal.options.disableStdin = false
  terminal.write('\r\n\x1b[32m[SSH connection restored]\x1b[0m\r\n')
  scheduleFit()
})

onBeforeUnmount(() => {
  if (resizeRegistrationTimer) clearTimeout(resizeRegistrationTimer)
  if (fitFrame !== null) cancelAnimationFrame(fitFrame)
  const remainingText = decoder.decode()
  if (remainingText && terminal) terminal.write(remainingText)
  unlistenData?.()
  unlistenDisconnected?.()
  resizeObserver?.disconnect()
  sessionStore.clearTerminalSelection(props.sessionId)
  terminal?.dispose()
})

function scheduleFit() {
  if (fitFrame !== null) return

  fitFrame = requestAnimationFrame(() => {
    fitFrame = null
    if (!fitAddon || !containerRef.value || containerRef.value.clientHeight === 0) return
    fitAddon.fit()
  })
}

function focus() {
  terminal?.focus()
}

function triggerResize() {
  scheduleFit()
}

defineExpose({ focus, triggerResize })
</script>

<template>
  <section class="terminal-shell">
    <div
      ref="containerRef"
      class="terminal-container"
      @click="focus"
    >
      <form v-if="searchVisible" class="terminal-search" @submit.prevent="runSearch(true)">
      <input
        ref="searchInputRef"
        v-model="searchQuery"
        type="search"
        spellcheck="false"
        placeholder="Search"
        @input="runSearch(true)"
        @keydown="handleSearchKeydown"
      />
      <button type="button" title="Previous match" aria-label="Previous match" @click="runSearch(false)"><ChevronUp :size="15" /></button>
      <button type="button" title="Next match" aria-label="Next match" @click="runSearch(true)"><ChevronDown :size="15" /></button>
      <button type="button" :class="{ active: searchCaseSensitive }" title="Match case" aria-label="Match case" @click="searchCaseSensitive = !searchCaseSensitive; runSearch(true)">Aa</button>
      <button type="button" :class="{ active: searchRegex }" title="Use regular expression" aria-label="Use regular expression" @click="searchRegex = !searchRegex; runSearch(true)"><Regex :size="15" /></button>
      <button type="button" title="Close search" aria-label="Close search" @click="closeSearch"><X :size="15" /></button>
      </form>
    </div>
    <div v-if="commandBarVisible" class="command-bar">
      <div v-if="historyVisible" class="command-history" role="listbox" aria-label="Command history">
        <div class="command-history-list">
          <button
            v-for="(entry, index) in commandHistory"
            :key="`${index}-${entry}`"
            type="button"
            :class="{ selected: index === historyIndex }"
            @click="useHistoryEntry(entry)"
          >{{ entry }}</button>
          <span v-if="!commandHistory.length" class="command-history-empty">No command history</span>
        </div>
        <footer>
          <span>Use Up/Down to select</span>
          <n-popconfirm positive-text="Clear" negative-text="Cancel" @positive-click="clearHistory">
            <template #trigger><button type="button">Clear history</button></template>
            Clear all command history?
          </n-popconfirm>
        </footer>
      </div>
      <form class="command-form" @submit.prevent="submitCommand">
        <input
          ref="commandInputRef"
          v-model="commandDraft"
          type="text"
          spellcheck="false"
          autocomplete="off"
          placeholder="Enter command"
          @keydown="handleCommandInputKeydown"
        >
      </form>
      <div class="command-actions">
        <button type="button" title="AI recognition" aria-label="AI recognition" disabled>AI recognition</button>
        <button type="button" title="Command history" aria-label="Command history" :class="{ active: historyVisible }" @click="toggleHistory"><History :size="16" /></button>
        <button v-if="isSsh" type="button" title="Reconnect" aria-label="Reconnect" @click="emit('reconnect')"><Zap :size="16" /></button>
        <button type="button" title="Close command input" aria-label="Close command input" @click="closeCommandBar"><X :size="16" /></button>
      </div>
    </div>
    <button v-else type="button" class="command-bar-open" title="Open command input" aria-label="Open command input" @click="openCommandBar()"><PanelBottomOpen :size="17" /></button>
  </section>
</template>

<style scoped>
.terminal-shell {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-terminal);
}

.terminal-container {
  box-sizing: border-box;
  width: 100%;
  height: auto;
  min-height: 0;
  flex: 1;
  position: relative;
  overflow: hidden;
  background: var(--app-terminal);
  padding: 0 8px;
}

.terminal-search {
  position: absolute;
  z-index: 2;
  top: 10px;
  right: 16px;
  display: flex;
  align-items: center;
  height: 30px;
  border: 1px solid #3b465d;
  border-radius: 5px;
  background: #1c2330;
  box-shadow: 0 4px 12px rgba(0, 0, 0, .25);
  overflow: hidden;
}

.terminal-search input {
  width: 190px;
  height: 100%;
  padding: 0 8px;
  border: 0;
  outline: 0;
  color: #d8deeb;
  background: transparent;
  font: 12px 'Cascadia Code', 'Fira Code', Consolas, monospace;
}

.terminal-search button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 100%;
  padding: 0;
  border: 0;
  border-left: 1px solid #3b465d;
  color: #9aa8be;
  background: transparent;
  cursor: pointer;
  font: 11px system-ui, sans-serif;
}

.terminal-search button:hover,
.terminal-search button.active {
  color: #d8deeb;
  background: #30394b;
}

.terminal-container :deep(.xterm) {
  height: 100%;
  overflow: hidden;
}

.terminal-container :deep(.xterm-viewport) {
  overflow-x: hidden;
  background: var(--app-terminal);
}

.command-bar {
  position: relative;
  display: flex;
  align-items: center;
  height: 38px;
  min-height: 38px;
  padding: 0 7px 0 10px;
  border-top: 1px solid var(--app-border);
  background: var(--app-surface);
  overflow: visible;
}

.command-form {
  min-width: 0;
  flex: 1;
}

.command-form input {
  box-sizing: border-box;
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 4px;
  outline: 0;
  color: var(--app-text);
  background: transparent;
  font: 12px 'Cascadia Code', 'Fira Code', Consolas, monospace;
}

.command-form input:focus {
  border-color: var(--app-accent);
  background: var(--app-base);
}

.command-form input::placeholder { color: var(--app-muted); }

.command-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
  margin-left: 5px;
}

.command-actions button,
.command-history footer button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 26px;
  min-width: 26px;
  padding: 0 6px;
  border: 0;
  border-radius: 4px;
  color: var(--app-muted);
  background: transparent;
  cursor: pointer;
  font: 12px system-ui, sans-serif;
}

.command-actions button:hover:not(:disabled),
.command-actions button.active,
.command-history footer button:hover { color: var(--app-text); background: var(--app-hover); }
.command-actions button:disabled { cursor: not-allowed; opacity: .55; }

.command-history {
  position: absolute;
  z-index: 4;
  right: 7px;
  bottom: calc(100% + 1px);
  display: flex;
  flex-direction: column;
  width: min(720px, calc(100vw - 32px));
  max-height: 340px;
  border: 1px solid var(--app-border);
  background: var(--app-surface);
  box-shadow: 0 -5px 18px var(--app-shadow);
}

.command-history-list {
  display: flex;
  min-height: 60px;
  overflow: auto;
  flex-direction: column;
  padding: 4px;
}

.command-history-list button {
  overflow: hidden;
  padding: 4px 7px;
  border: 0;
  color: var(--app-text);
  background: transparent;
  cursor: pointer;
  font: 12px 'Cascadia Code', 'Fira Code', Consolas, monospace;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.command-history-list button:hover,
.command-history-list button.selected { background: var(--app-hover); }
.command-history-empty { padding: 10px; color: var(--app-muted); font-size: 12px; }

.command-bar-open {
  position: absolute;
  z-index: 3;
  right: 12px;
  bottom: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--app-border);
  border-radius: 4px;
  color: var(--app-muted);
  background: var(--app-surface);
  cursor: pointer;
  opacity: .55;
  transition: opacity .15s, color .15s, background .15s;
}

.command-bar-open:hover,
.command-bar-open:focus-visible {
  color: var(--app-text);
  background: var(--app-hover);
  opacity: 1;
}

.command-history footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 34px;
  padding: 0 5px 0 10px;
  border-top: 1px solid var(--app-border);
  color: var(--app-muted);
  font-size: 11px;
}

.command-history footer button { color: var(--app-text); }
</style>
