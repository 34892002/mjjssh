<script setup lang="ts">
import { nextTick, ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { ChevronDown, ChevronUp, Regex, X } from '@lucide/vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import '@xterm/xterm/css/xterm.css'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSessionStore } from '../stores/session'

const props = defineProps<{
  sessionId: string
  dark: boolean
  reconnectVersion?: number
}>()

const emit = defineEmits<{
  disconnected: [reason: string]
}>()

const containerRef = ref<HTMLDivElement | null>(null)
const searchInputRef = ref<HTMLInputElement | null>(null)
const searchVisible = ref(false)
const searchQuery = ref('')
const searchCaseSensitive = ref(false)
const searchRegex = ref(false)
const sessionStore = useSessionStore()

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
  terminal.write(`\r\n\x1b[31m[SSH terminal unavailable: ${message}]\x1b[0m\r\n`)
}

async function writeTerminalData(data: string) {
  if (terminalUnavailable) return
  if (!await sessionStore.writeData(props.sessionId, data)) {
    markTerminalUnavailable('input could not be delivered; reconnect to continue.')
  }
}

onMounted(async () => {
  if (!containerRef.value) return

  terminal = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    scrollback: 5000,
    fontSize: 14,
    fontFamily: '"Cascadia Code", "Fira Code", "JetBrains Mono", Consolas, monospace',
    theme: terminalTheme(),
  })

  fitAddon = new FitAddon()
  searchAddon = new SearchAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(searchAddon)
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'

  terminal.open(containerRef.value)

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

  // 注册 SSH 数据监听
  unlistenData = await listen<number[]>(`ssh-data:${props.sessionId}`, (event) => {
    const bytes = new Uint8Array(event.payload)
    const text = decoder.decode(bytes, { stream: true })
    if (text) terminal.write(text)
  })
  unlistenDisconnected = await listen<string>(`ssh-disconnected:${props.sessionId}`, (event) => {
    markTerminalUnavailable(`connection disconnected: ${event.payload}`)
    emit('disconnected', event.payload)
  })
  sessionStore.notifyTerminalReady(props.sessionId)
})

watch(() => props.dark, () => {
  if (terminal) terminal.options.theme = terminalTheme()
})

watch(() => props.reconnectVersion, (version, previousVersion) => {
  if (version === undefined || version === previousVersion || !terminalUnavailable) return
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
</template>

<style scoped>
.terminal-container {
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  position: relative;
  overflow: hidden;
  background: var(--app-terminal);
  padding: 5px 8px;
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
</style>
