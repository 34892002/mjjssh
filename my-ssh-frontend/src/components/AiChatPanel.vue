<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { readText } from '@tauri-apps/plugin-clipboard-manager'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { NDropdown, NNumberAnimation, type DropdownOption } from 'naive-ui'
import { AlertCircle, ArrowUp, CheckCircle2, ChevronDown, Command, Download, History, LoaderCircle, Plus, Sparkles, X, XCircle } from '@lucide/vue'
import { useAiStore, type AiActionRecord } from '../stores/ai'
import { useSessionStore } from '../stores/session'
import type { AiActionResult, AiExecutableDecision, AiExecutableGrant, AiExecutionMode, AiImageInput, AiPendingAction, AiStreamEvent, AiTaskStarted, AiTerminalSelection, StartAiTaskInput } from '../types/ai'

const props = defineProps<{ sessionId: string }>()
const emit = defineEmits<{ close: []; openAiSettings: [] }>()

const aiStore = useAiStore()
const sessionStore = useSessionStore()
const draft = ref('')
const pendingImages = ref<AiImageInput[]>([])
const pendingTerminalSelection = ref<AiTerminalSelection | null>(null)
const previewImage = ref<AiImageInput | null>(null)
const messages = computed({
  get: () => aiStore.getConversation(props.sessionId),
  set: (value) => aiStore.setConversation(props.sessionId, value),
})
const executionMode = ref<AiExecutionMode>('read_only')
const showExecutionModeMenu = ref(false)
const executionModeDetails: Record<AiExecutionMode, { label: string; description: string; tone: string; riskLabel: string }> = {
  read_only: {
    label: '问答模式',
    description: '问答对话，不具备执行命令权限',
    tone: '#5bd8aa',
    riskLabel: '安全',
  },
  approval_required: {
    label: '确认模式',
    description: '执行命令需要用户授权',
    tone: '#8fb2ee',
    riskLabel: '可控',
  },
  autonomous: {
    label: '自动模式',
    description: '获得全权授权，自主执行命令',
    tone: '#e4b85d',
    riskLabel: '风险',
  },
}
const executionModeOptions = computed<DropdownOption[]>(() =>
  (Object.entries(executionModeDetails) as [AiExecutionMode, typeof executionModeDetails[AiExecutionMode]][])
    .map(([key, mode]) => ({
      key,
      type: 'render',
      render: () => h('button', {
        type: 'button',
        class: ['execution-mode-option', { selected: executionMode.value === key }],
        onClick: () => selectExecutionMode(key),
      }, [
        h('span', { class: 'execution-mode-option-copy' }, [
          h('span', { class: 'execution-mode-option-heading' }, [
            h('span', { class: 'execution-mode-option-title' }, mode.label),
            h('span', { class: 'execution-mode-option-risk', style: { color: mode.tone } }, mode.riskLabel),
          ]),
          h('span', { class: 'execution-mode-option-description' }, mode.description),
        ]),
      ]),
    })),
)
const currentExecutionMode = computed(() => executionModeDetails[executionMode.value])
const modelOptions = computed<DropdownOption[]>(() =>
  (aiStore.config.models ?? []).map((model) => ({
    key: model.id,
    type: 'render',
    render: () => h('button', {
      type: 'button',
      class: ['model-option', { selected: activeModel.value?.id === model.id }],
      onClick: () => selectModel(model.id),
    }, [
      h('span', { class: 'model-option-name' }, model.name),
      h('span', { class: 'model-option-meta' }, model.protocol === 'responses' ? 'Responses API' : 'Chat Completions'),
    ]),
  })),
)
const requestId = ref<string | null>(null)
const taskError = ref<string | null>(null)
const pendingActionPlan = ref<string | null>(null)
const pendingAction = ref<AiPendingAction | null>(null)
type ActionBubble = AiActionRecord

const actionBubble = ref<ActionBubble | null>(null)
const actionHistory = computed({
  get: () => aiStore.getActionHistory(props.sessionId),
  set: (value) => aiStore.setActionHistory(props.sessionId, value),
})
const elapsedNow = ref(Date.now())
const decidingAction = ref(false)
const executableDecisions = ref<Record<string, AiExecutableGrant | undefined>>({})
const taskResponseMessageId = ref<string | null>(null)
const taskUserMessageId = ref<string | null>(null)
const taskExecutionMode = ref<AiExecutionMode>('read_only')
const taskContent = ref('')
const taskStatus = ref<string | null>(null)
let unlistenTask: UnlistenFn | null = null
let unlistenSshDisconnected: UnlistenFn | null = null

const bodyRef = ref<HTMLElement | null>(null)
const shouldFollowOutput = ref(true)
const showHistory = ref(false)
const showAgentMenu = ref(false)
const showModelMenu = ref(false)

const conversationHistory = computed(() => aiStore.getConversationHistory(props.sessionId))
const selectedAgent = computed(() => aiStore.agents.find((agent) => agent.id === aiStore.selectedAgentId) ?? null)
const activeModel = computed(() => aiStore.config.models?.find((model) => model.id === aiStore.config.activeModelId) ?? null)
const activeModelLabel = computed(() => activeModel.value?.name ?? aiStore.config.model ?? '未配置模型')
const canSend = computed(() => Boolean(draft.value.trim() || pendingImages.value.length || pendingTerminalSelection.value) && aiStore.config.configured && selectedAgent.value && activeModel.value)
const isWaitingForResponse = computed(() =>
  Boolean(requestId.value) && messages.value.at(-1)?.role !== 'assistant',
)
const timelineMessages = computed(() => messages.value.filter(
  (message) => message.id !== taskResponseMessageId.value,
))
const taskResponseMessage = computed(() => messages.value.find(
  (message) => message.id === taskResponseMessageId.value,
) ?? null)
const taskErrorMessage = computed(() => taskError.value)

async function selectModel(modelId: string) {
  const model = aiStore.config.models?.find((item) => item.id === modelId)
  if (!model || model.id === aiStore.config.activeModelId || aiStore.loading) {
    showModelMenu.value = false
    return
  }

  try {
    await aiStore.saveConfig({
      baseUrl: aiStore.config.baseUrl ?? '',
      apiKey: '',
      model: model.name,
      models: aiStore.config.models ?? [],
      activeModelId: model.id,
      timeoutSeconds: aiStore.config.timeoutSeconds ?? 60,
    })
  } catch {
    // The store retains the provider error for the existing request-error surface.
  } finally {
    showModelMenu.value = false
  }
}
const autoRetryStatus = computed(() => {
  const status = taskStatus.value
  const marker = '可点击停止取消'
  if (!status?.includes('正在自动重试') || !status.includes(marker)) return null
  const [before] = status.split(marker)
  return { before, after: '取消' }
})
const completedActionsByMessage = computed(() => {
  const actions = new Map<string, ActionBubble[]>()
  for (const action of actionHistory.value) {
    const records = actions.get(action.messageId) ?? []
    records.push(action)
    actions.set(action.messageId, records)
  }
  return actions
})
const taskHasStartedActions = computed(() =>
  actionHistory.value.some((action) => action.messageId === taskUserMessageId.value),
)

function renderMarkdown(content: string) {
  return DOMPurify.sanitize(marked.parse(content, { async: false }) as string)
}

function selectExecutionMode(key: string | number) {
  if (key in executionModeDetails) {
    executionMode.value = key as AiExecutionMode
    showExecutionModeMenu.value = false
  }
}

function handleBodyScroll() {
  const body = bodyRef.value
  if (!body) return
  shouldFollowOutput.value = body.scrollHeight - body.scrollTop - body.clientHeight < 24
}

async function scrollToLatest(force = false) {
  if (!force && !shouldFollowOutput.value) return
  await nextTick()
  const body = bodyRef.value
  if (body) body.scrollTop = body.scrollHeight
}

watch([messages, requestId, actionBubble, actionHistory], () => void scrollToLatest(), { deep: true, flush: 'post' })

let elapsedTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await Promise.all([aiStore.loadConfigStatus(), aiStore.loadAgents()])
  unlistenSshDisconnected = await listen<string>(`ssh-disconnected:${props.sessionId}`, () => {
    if (requestId.value) {
      taskError.value = 'SSH 连接已断开，AI 任务已停止。'
      requestId.value = null
      decidingAction.value = false
      unlistenTask?.()
      unlistenTask = null
    }
  })
  elapsedTimer = setInterval(() => { elapsedNow.value = Date.now() }, 100)
})

function actionStatusLabel(status: ActionBubble['status']) {
  return {
    awaiting_authorization: '等待授权',
    awaiting_risk_confirmation: '等待风险确认',
    executing: '等待 SSH 完成确认',
    completed: '执行完成',
    failed: '命令返回非零状态',
    unconfirmed: '未确认命令完成',
    terminal_blocked: 'SSH 终端不可用',
    recovery_failed: '终端恢复未确认',
    rejected: '已拒绝',
  }[status]
}

function actionElapsedSeconds(bubble: ActionBubble) {
  const end = bubble.finishedAt ?? elapsedNow.value
  return Math.max(0, (end - bubble.createdAt) / 1000)
}

function actionElapsed(bubble: ActionBubble) {
  const seconds = actionElapsedSeconds(bubble)
  return seconds < 1 ? '< 1 秒' : `${seconds.toFixed(1)} 秒`
}

function isCompletedActionStatus(status: ActionBubble['status']) {
  return status === 'completed' || status === 'failed' || status === 'unconfirmed' || status === 'terminal_blocked' || status === 'recovery_failed' || status === 'rejected'
}

function truncateEvidence(value: string, limit: number) {
  const compact = value.replace(/\s+/g, ' ').trim()
  return compact.length <= limit ? compact : `${compact.slice(0, limit)}...`
}

function actionEvidenceMessage(history: ActionBubble[]) {
  const completedActions = history.filter((action) => isCompletedActionStatus(action.status)).slice(-3)
  if (!completedActions.length) return null

  return [
    '本次会话中已执行操作的系统记录如下。请基于这些记录回答用户，不要声称未记录的命令或结果。',
    ...completedActions.map((action, index) => [
      `${index + 1}. 状态：${actionStatusLabel(action.status)}`,
      `命令：${truncateEvidence(action.action.command, 240)}`,
      `目的：${truncateEvidence(action.action.purpose, 160)}`,
      `结果：${truncateEvidence(action.result?.summary ?? '未返回结果摘要', 500)}`,
    ].join('\n')),
  ].join('\n\n')
}

function toggleActionDetails(bubble: ActionBubble) {
  if (isCompletedActionStatus(bubble.status)) bubble.collapsed = !bubble.collapsed
}



async function sendMessage() {
  const content = draft.value.trim()
  const terminalSelection = pendingTerminalSelection.value
  if ((!content && !pendingImages.value.length && !terminalSelection) || !aiStore.config.configured || !selectedAgent.value || requestId.value) return
  if (pendingImages.value.length && !activeModel.value?.supportsImages) {
    taskError.value = '当前模型未启用图片输入，请在模型配置中开启后重试'
    return
  }
  await startTask(content || '请分析以下终端选区。', executionMode.value, pendingImages.value, terminalSelection ? [terminalSelection] : [])
  draft.value = ''
  pendingImages.value = []
  pendingTerminalSelection.value = null
}

async function retryAiRequest() {
  if (!taskError.value || requestId.value) return
  const executionHasStarted = taskHasStartedActions.value
  const content = executionHasStarted
    ? '请基于本次会话中已有的 SSH 操作记录继续分析并回答，不要执行任何 SSH 命令。'
    : taskContent.value
  const mode = executionHasStarted ? 'read_only' : taskExecutionMode.value
  await startTask(content, mode)
}

function terminalSelectionContext(selections: AiTerminalSelection[]) {
  return selections.map((selection) => `\n\n[Terminal selection, ${selection.lineCount} lines]\n\`\`\`terminal\n${selection.text}\n\`\`\``).join('')
}

function messageForRequest(message: { role: 'user' | 'assistant'; content: string; images?: AiImageInput[]; terminalSelections?: AiTerminalSelection[] }) {
  return {
    role: message.role,
    content: `${message.content}${terminalSelectionContext(message.terminalSelections ?? [])}`,
    images: message.images,
  }
}

async function startTask(
  content: string,
  mode: AiExecutionMode,
  images: AiImageInput[] = [],
  terminalSelections: AiTerminalSelection[] = [],
) {
  const userMessage = { id: crypto.randomUUID(), role: 'user' as const, content, images, terminalSelections }
  const conversation = [...messages.value, userMessage]
  messages.value = conversation
  shouldFollowOutput.value = true
  void scrollToLatest(true)
  taskError.value = null
  pendingActionPlan.value = null
  pendingAction.value = null
  actionBubble.value = null
  executableDecisions.value = {}
  taskResponseMessageId.value = null
  taskUserMessageId.value = userMessage.id
  taskExecutionMode.value = mode
  taskContent.value = content
  taskStatus.value = 'AI 正在规划检查步骤'

  const nextRequestId = crypto.randomUUID()
  requestId.value = nextRequestId
  unlistenTask = await listen<AiStreamEvent>(`ai-task:${nextRequestId}`, ({ payload }) => {
    if (payload.eventType === 'delta' && payload.content) {
      let targetMessage = taskResponseMessageId.value
        ? messages.value.find((message) => message.id === taskResponseMessageId.value)
        : undefined
      if (!targetMessage) {
        const id = crypto.randomUUID()
        targetMessage = { id, role: 'assistant', content: '' }
        messages.value.push(targetMessage)
        taskResponseMessageId.value = id
      }
      targetMessage.content += payload.content
    }
    if (payload.eventType === 'task_status' && payload.content) {
      taskStatus.value = payload.content
      if (actionBubble.value?.status === 'executing') actionBubble.value.phase = payload.content
    }
    if (payload.eventType === 'plan') pendingActionPlan.value = payload.content ?? null
    if (payload.eventType === 'action_pending' && payload.action) {
      pendingAction.value = payload.action
      actionBubble.value = {
        messageId: taskUserMessageId.value ?? userMessage.id,
        action: payload.action,
        plan: pendingActionPlan.value ?? undefined,
        status: 'awaiting_authorization',
        createdAt: Date.now(),
      }
      actionHistory.value = [...actionHistory.value, actionBubble.value]
      pendingActionPlan.value = null
      executableDecisions.value = Object.fromEntries(payload.action.missingExecutables.map((executable) => [executable, undefined]))
    }
    if (payload.eventType === 'risk_confirmation_required' && payload.action) {
      if (payload.content) {
        const id = crypto.randomUUID()
        messages.value.push({ id, role: 'assistant', content: payload.content })
        taskResponseMessageId.value = id
      }
      pendingAction.value = null
      const bubble = {
        messageId: taskUserMessageId.value ?? userMessage.id,
        action: payload.action,
        plan: pendingActionPlan.value ?? undefined,
        status: 'awaiting_risk_confirmation' as const,
        createdAt: Date.now(),
        collapsed: false,
        result: {
          actionId: payload.action.actionId,
          status: 'awaiting_risk_confirmation' as const,
          summary: '尚未执行。确认仅对此命令和当前 SSH 会话有效。',
        },
      }
      actionHistory.value = [...actionHistory.value, bubble]
      pendingActionPlan.value = null
    }
    if (payload.eventType === 'action_started' && payload.action) {
      let bubble = actionHistory.value.find((entry) => entry.action.actionId === payload.action?.actionId)
      if (!bubble) {
        bubble = {
          messageId: taskUserMessageId.value ?? userMessage.id,
          action: payload.action,
          plan: pendingActionPlan.value ?? undefined,
          status: 'executing',
          createdAt: Date.now(),
          startedAt: Date.now(),
          phase: taskStatus.value ?? '命令已发送，等待 SSH 返回退出状态',
        }
        actionHistory.value.push(bubble)
        pendingActionPlan.value = null
      } else {
        bubble.plan ??= pendingActionPlan.value ?? undefined
        bubble.status = 'executing'
        bubble.startedAt = Date.now()
        bubble.phase = taskStatus.value ?? '命令已发送，等待 SSH 返回退出状态'
      }
      actionBubble.value = bubble
      pendingAction.value = null
    }
    if (payload.eventType === 'action_completed' && payload.actionResult) {
      pendingAction.value = null
      decidingAction.value = false
      const bubble = actionHistory.value.find((entry) => entry.action.actionId === payload.actionResult?.actionId)
      if (bubble) {
        bubble.status = payload.actionResult.status
        bubble.finishedAt = Date.now()
        bubble.collapsed = true
        bubble.result = payload.actionResult
        bubble.phase = undefined
        actionBubble.value = bubble
      }
    }
    if (payload.eventType === 'policy_rejected') {
      taskError.value = payload.content ?? '命令未通过安全策略，未执行'
    }
    if (payload.eventType === 'error') {
      taskError.value = payload.content ?? 'AI 请求失败'
    }
    if (payload.eventType === 'completed' || payload.eventType === 'cancelled' || payload.eventType === 'error' || payload.eventType === 'policy_rejected') {
      requestId.value = null
      decidingAction.value = false
      unlistenTask?.()
      unlistenTask = null
    }
  })

  const input: StartAiTaskInput = {
    requestId: nextRequestId,
    sessionId: props.sessionId,
    conversationId: 'current',
    agentId: selectedAgent.value?.id,
    model: activeModel.value?.id,
    messages: [
      ...conversation.slice(0, -1).map(messageForRequest),
      ...([actionEvidenceMessage(actionHistory.value)].filter((content): content is string => Boolean(content))
        .map((content) => ({ role: 'user' as const, content }))),
      ...conversation.slice(-1).map(({ role, content, images }) => ({ role, content, images })),
    ],
    executionMode: mode,
    scopes: mode === 'read_only'
      ? ['read_only_diagnostics']
      : ['command_execution'],
    includeTerminalContext: terminalSelections.length > 0,
    terminalContext: terminalSelections.map((selection) => selection.text).join('\n'),
  }

  try {
    await invoke<AiTaskStarted>('start_ai_task', { input })
  } catch (error) {
    requestId.value = null
    unlistenTask?.()
    unlistenTask = null
    messages.value.pop()
    taskError.value = String(error)
  }
}

function selectAgent(agentId: string) {
  aiStore.selectAgent(agentId)
  showAgentMenu.value = false
}



async function cancelTask() {
  if (!requestId.value) return
  await invoke('cancel_ai_task', { requestId: requestId.value })
}

function setExecutableDecision(executable: string, grant: AiExecutableGrant) {
  executableDecisions.value[executable] = grant
}

function setAllExecutableDecisions(grant: AiExecutableGrant) {
  if (!pendingAction.value || decidingAction.value) return
  for (const executable of pendingAction.value.missingExecutables) {
    executableDecisions.value[executable] = grant
  }
}

const hasCompleteExecutableDecisions = computed(() =>
  pendingAction.value?.missingExecutables.every((executable) => executableDecisions.value[executable]) ?? false,
)

async function decidePendingAction() {
  if (!requestId.value || !pendingAction.value || decidingAction.value || !hasCompleteExecutableDecisions.value) return
  decidingAction.value = true
  const decisions: AiExecutableDecision[] = pendingAction.value.missingExecutables.map((executable) => ({
    executable,
    grant: executableDecisions.value[executable]!,
  }))
  try {
    await invoke('decide_ai_action', {
      input: { requestId: requestId.value, actionId: pendingAction.value.actionId, decisions },
    })
  } catch (error) {
    taskError.value = String(error)
    decidingAction.value = false
  }
}

async function confirmRiskAction(bubble: ActionBubble) {
  const riskConfirmationId = bubble.action.riskConfirmationId
  if (!riskConfirmationId || decidingAction.value || bubble.status !== 'awaiting_risk_confirmation') return

  decidingAction.value = true
  bubble.status = 'executing'
  bubble.startedAt = Date.now()
  bubble.phase = '命令已发送，等待 SSH 返回退出状态'
  try {
    const result = await invoke<AiActionResult>('confirm_ai_risk_action', {
      input: {
        sessionId: props.sessionId,
        actionId: bubble.action.actionId,
        riskConfirmationId,
      },
    })
    bubble.status = result.status
    bubble.result = result
    bubble.finishedAt = Date.now()
    bubble.collapsed = isCompletedActionStatus(result.status)
    bubble.phase = undefined
  } catch (error) {
    bubble.status = 'awaiting_risk_confirmation'
    bubble.startedAt = undefined
    bubble.phase = undefined
    taskError.value = String(error)
  } finally {
    decidingAction.value = false
  }
}

function startNewConversation() {
  if (requestId.value) return
  aiStore.startNewConversation(props.sessionId)
  showHistory.value = false
  taskError.value = null
  draft.value = ''
  pendingImages.value = []
  pendingTerminalSelection.value = null
  shouldFollowOutput.value = true
}

function selectConversation(conversationId: string) {
  aiStore.selectConversation(props.sessionId, conversationId)
  showHistory.value = false
  taskError.value = null
  shouldFollowOutput.value = true
  void scrollToLatest(true)
}

function conversationPreview(messages: Array<{ content: string }>) {
  return messages[0]?.content.replace(/\s+/g, ' ').slice(0, 36) ?? '新对话'
}

function formatConversationTime(createdAt: number) {
  return new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit' }).format(createdAt)
}

function appendToDraft(text: string) {
  if (!text) return
  draft.value = draft.value
    ? `${draft.value}${draft.value.endsWith('\n') ? '' : '\n'}${text}`
    : text
}

async function handlePaste(event: ClipboardEvent) {
  event.preventDefault()
  const image = Array.from(event.clipboardData?.files ?? []).find((file) => file.type.startsWith('image/'))
  if (image) {
    if (!activeModel.value?.supportsImages) {
      taskError.value = '当前模型未启用图片输入，请在模型配置中开启后重试'
      return
    }
    if (image.size > 6 * 1024 * 1024) {
      taskError.value = '图片不能超过 6 MiB'
      return
    }
    const dataUrl = await readImageDataUrl(image)
    pendingImages.value = [...pendingImages.value, { dataUrl, name: image.name || 'image.png' }].slice(0, 4)
    taskError.value = null
    return
  }

  const text = event.clipboardData?.getData('text/plain')
  if (text) addPastedText(text)
}

function addPastedText(text: string) {
  const selection = sessionStore.consumeTerminalSelection(props.sessionId, text)
  if (selection) {
    pendingTerminalSelection.value = { text: selection.text, lineCount: selection.lineCount }
    taskError.value = null
    return
  }
  appendToDraft(text)
}

function removePendingTerminalSelection() {
  pendingTerminalSelection.value = null
}

function readImageDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error)
    reader.onload = () => resolve(String(reader.result))
    reader.readAsDataURL(file)
  })
}

function removePendingImage(index: number) {
  pendingImages.value.splice(index, 1)
}

function openImagePreview(image: AiImageInput) {
  previewImage.value = image
}

async function pasteClipboardIntoDraft() {
  try {
    addPastedText(await readText())
  } catch (error) {
    taskError.value = `读取剪贴板失败：${String(error)}`
  }
}

onBeforeUnmount(() => {
  if (requestId.value) void cancelTask()
  unlistenTask?.()
  unlistenSshDisconnected?.()
  if (elapsedTimer) clearInterval(elapsedTimer)
})
</script>

<template>
  <section class="ai-chat-panel">
    <header class="ai-header">
      <div class="agent-menu-wrap">
        <button class="agent-selector" title="选择 AI 助手" @click="showAgentMenu = !showAgentMenu">
          <Sparkles :size="15" />
          <strong>{{ selectedAgent?.name ?? '选择 Agent' }}</strong>
          <ChevronDown :size="14" />
        </button>
        <div v-if="showAgentMenu" class="agent-menu">
          <button v-for="agent in aiStore.agents" :key="agent.id" :class="{ selected: agent.id === selectedAgent?.id }" @click="selectAgent(agent.id)">
            <span>{{ agent.name }}</span><small v-if="agent.isDefault">默认</small>
          </button>

        </div>
      </div>
      <div class="header-actions">
        <button title="导出会话"><Download :size="15" /></button>
        <button title="会话历史" @click="showHistory = !showHistory"><History :size="15" /></button>
        <button title="新建对话" :disabled="Boolean(requestId)" @click="startNewConversation"><Plus :size="17" /></button>
        <button class="close-button" title="关闭 AI 对话" @click="emit('close')"><X :size="16" /></button>
      </div>
    </header>

    <div ref="bodyRef" class="ai-body" @scroll="handleBodyScroll">
      <section v-if="showHistory" class="conversation-history">
        <div class="section-heading"><span>历史对话</span><button @click="showHistory = false">关闭</button></div>
        <p v-if="!conversationHistory.length" class="history-empty">暂无历史对话</p>
        <button
          v-for="conversation in conversationHistory"
          :key="conversation.id"
          class="history-item"
          :class="{ active: conversation.messages === messages }"
          @click="selectConversation(conversation.id)"
        >
          <span>{{ conversationPreview(conversation.messages) }}</span>
          <time>{{ formatConversationTime(conversation.createdAt) }}</time>
        </button>
      </section>

      <div v-else-if="!messages.length" class="ai-empty-state">
        <template v-if="aiStore.loading"><p>正在读取 AI 配置…</p></template>
        <p v-if="!aiStore.config.configured">请前往“设置 → AI”配置服务后再开始对话。</p>
        <p v-else>AI 会基于你提供的 SSH 上下文分析问题，并给出需要你确认和执行的诊断建议。</p>
      </div>
      <div v-else class="ai-messages">
        <template v-for="message in timelineMessages" :key="message.id">
          <article class="message" :class="message.role">
            <div v-if="message.role === 'assistant'" class="message-content" v-html="renderMarkdown(message.content)" />
            <div v-else class="message-content"><div v-if="message.content">{{ message.content }}</div><div v-if="message.terminalSelections?.length" class="message-terminal-selections"><span v-for="selection in message.terminalSelections" :key="selection.text" class="terminal-selection-chip">终端选区 · {{ selection.lineCount }} 行</span></div><div v-if="message.images?.length" class="message-images"><img v-for="image in message.images" :key="image.dataUrl" :src="image.dataUrl" :alt="image.name" /></div></div>
          </article>
          <section v-for="bubble in completedActionsByMessage.get(message.id) ?? []" :key="bubble.action.actionId" class="ai-action-card" :class="[bubble.status, { collapsed: bubble.collapsed }]">
          <button
            v-if="bubble.collapsed"
            type="button"
            class="completed-action-row"
            :class="bubble.status"
            :title="`展开执行详情：${bubble.action.command}`"
            @click="toggleActionDetails(bubble)"
          >
            <ChevronDown :size="15" class="collapsed" />
            <CheckCircle2 v-if="bubble.status === 'completed'" :size="15" />
            <AlertCircle v-else-if="bubble.status === 'unconfirmed' || bubble.status === 'terminal_blocked' || bubble.status === 'recovery_failed'" :size="15" />
            <XCircle v-else :size="15" />
            <code>{{ bubble.action.command }}</code>
            <span>执行记录 · {{ actionElapsed(bubble) }} · {{ bubble.action.riskLevel === 'autonomous' ? '自主执行' : `${bubble.action.riskLevel} 风险` }}</span>
          </button>
          <Transition name="action-details">
            <div v-if="!bubble.collapsed" class="action-details">
              <div class="action-details-inner">
            <div class="action-card-heading">
              <div class="action-status">
                <LoaderCircle v-if="bubble.status === 'awaiting_authorization' || bubble.status === 'executing'" :size="15" class="status-spinner" />
                <CheckCircle2 v-else-if="bubble.status === 'completed'" :size="15" />
                <AlertCircle v-else-if="bubble.status === 'unconfirmed' || bubble.status === 'terminal_blocked' || bubble.status === 'recovery_failed'" :size="15" />
                <XCircle v-else :size="15" />
                <strong>{{ actionStatusLabel(bubble.status) }}</strong>
              </div>
              <div class="action-card-meta">
                <span>
                  <template v-if="bubble.status === 'executing'">
                    <NNumberAnimation :from="Math.max(0, actionElapsedSeconds(bubble) - 0.1)" :to="actionElapsedSeconds(bubble)" :precision="1" :duration="90" /> 秒
                  </template>
                  <template v-else>执行记录 · {{ actionElapsed(bubble) }}</template>
                  · {{ bubble.action.riskLevel === 'autonomous' ? '自主执行' : `${bubble.action.riskLevel} 风险` }}
                </span>
                <button
                  v-if="isCompletedActionStatus(bubble.status)"
                  type="button"
                  class="action-collapse-button"
                  title="收起执行详情"
                  aria-label="收起执行详情"
                  @click="toggleActionDetails(bubble)"
                >
                  <ChevronDown :size="15" />
                </button>
              </div>
            </div>
            <div class="action-output">
              <p v-if="bubble.plan" class="action-plan">{{ bubble.plan }}</p>
              <code>{{ bubble.action.command }}</code>
            </div>
            <dl>
            <div><dt>目的</dt><dd>{{ bubble.action.purpose }}</dd></div>
            <div><dt>影响</dt><dd>{{ bubble.action.expectedImpact }}</dd></div>
              <div><dt>回滚</dt><dd>{{ bubble.action.rollbackHint }}</dd></div>
            </dl>
            <p v-if="bubble.status === 'awaiting_risk_confirmation'" class="action-result">尚未执行。确认仅对此命令和当前 SSH 会话有效。</p>
            <p v-else-if="bubble.result" class="action-result">{{ bubble.result.summary }}</p>
              </div>
            </div>
          </Transition>
          <template v-if="bubble.status === 'awaiting_authorization'">
            <div class="executable-authorizations">
              <div v-for="executable in bubble.action.missingExecutables" :key="executable" class="executable-authorization">
                <code>{{ executable }}</code>
                <div class="grant-options" role="group" :aria-label="`${executable} 授权范围`">
                  <button type="button" :class="{ selected: executableDecisions[executable] === 'once' }" :disabled="decidingAction" @click="setExecutableDecision(executable, 'once')">仅此一次</button>
                  <button type="button" :class="{ selected: executableDecisions[executable] === 'server' }" :disabled="decidingAction" @click="setExecutableDecision(executable, 'server')">此服务器</button>
                  <button type="button" :class="{ selected: executableDecisions[executable] === 'global' }" :disabled="decidingAction" @click="setExecutableDecision(executable, 'global')">所有服务器</button>
                  <button type="button" class="reject" :class="{ selected: executableDecisions[executable] === 'reject' }" :disabled="decidingAction" @click="setExecutableDecision(executable, 'reject')">拒绝</button>
                </div>
              </div>
            </div>
            <div class="action-card-buttons authorization-actions">
              <label class="bulk-grant-select">
                <span>统一授权</span>
                <select :disabled="decidingAction" @change="setAllExecutableDecisions(($event.target as HTMLSelectElement).value as AiExecutableGrant)">
                  <option value="" selected disabled>选择范围</option>
                  <option value="once">仅此一次</option>
                  <option value="server">此服务器</option>
                  <option value="global">所有服务器</option>
                  <option value="reject">全部拒绝</option>
                </select>
              </label>
              <button type="button" class="approve" :disabled="decidingAction || !hasCompleteExecutableDecisions" @click="decidePendingAction">提交授权并执行</button>
            </div>
          </template>
          <div v-else-if="bubble.status === 'awaiting_risk_confirmation'" class="action-card-buttons">
            <button type="button" class="approve" :disabled="decidingAction" @click="confirmRiskAction(bubble)">同意执行</button>
          </div>
          <div v-else-if="bubble.status === 'executing'" class="action-progress">
            <span>{{ bubble.phase ?? '命令已发送，等待 SSH 返回退出状态' }}</span>
          </div>
          </section>
        </template>
        <article v-if="taskResponseMessage" :key="taskResponseMessage.id" class="message assistant">
          <div class="message-content" v-html="renderMarkdown(taskResponseMessage.content)" />
        </article>
        <div v-if="isWaitingForResponse && !pendingAction && actionBubble?.status !== 'executing'" class="thinking-indicator" :aria-label="taskStatus ?? 'AI 正在分析'">
          <svg class="thinking-squares" viewBox="0 0 15 15" shape-rendering="crispEdges" aria-hidden="true">
            <rect x="0" y="0" width="3" height="3">
              <animate attributeName="x" values="0;5;10;10;10;5;0;0;0" dur=".48s" repeatCount="indefinite" calcMode="discrete" />
              <animate attributeName="y" values="0;0;0;5;10;10;10;5;0" dur=".48s" repeatCount="indefinite" calcMode="discrete" />
            </rect>
            <rect x="0" y="0" width="3" height="3">
              <animate attributeName="x" values="0;5;10;10;10;5;0;0;0" dur=".48s" begin="-.06s" repeatCount="indefinite" calcMode="discrete" />
              <animate attributeName="y" values="0;0;0;5;10;10;10;5;0" dur=".48s" begin="-.06s" repeatCount="indefinite" calcMode="discrete" />
            </rect>
            <rect x="0" y="0" width="3" height="3">
              <animate attributeName="x" values="0;5;10;10;10;5;0;0;0" dur=".48s" begin="-.12s" repeatCount="indefinite" calcMode="discrete" />
              <animate attributeName="y" values="0;0;0;5;10;10;10;5;0" dur=".48s" begin="-.12s" repeatCount="indefinite" calcMode="discrete" />
            </rect>
            <rect x="0" y="0" width="3" height="3">
              <animate attributeName="x" values="0;5;10;10;10;5;0;0;0" dur=".48s" begin="-.18s" repeatCount="indefinite" calcMode="discrete" />
              <animate attributeName="y" values="0;0;0;5;10;10;10;5;0" dur=".48s" begin="-.18s" repeatCount="indefinite" calcMode="discrete" />
            </rect>
          </svg>
          <span v-if="autoRetryStatus" class="thinking-label">{{ autoRetryStatus.before }}<button type="button" class="task-inline-action" @click="cancelTask">停止</button>{{ autoRetryStatus.after }}</span>
          <span v-else class="thinking-label">{{ taskStatus ?? 'AI 正在分析' }}</span>
        </div>
        <section v-if="taskErrorMessage" class="task-error-card">
          <div>
            <AlertCircle :size="16" />
            <strong>AI 请求失败</strong>
          </div>
          <p>系统已自动重试 3 次。{{ taskHasStartedActions ? '将基于已有执行记录继续分析，不会重复执行 SSH 命令。' : '' }}<button type="button" class="task-inline-action" @click="retryAiRequest">重试 AI 请求</button></p>
          <p>{{ taskErrorMessage }}</p>
        </section>
      </div>

      <section v-if="!showHistory && !messages.length && conversationHistory.length" class="recent-section">
        <div class="section-heading"><span>最近</span><button @click="showHistory = true">查看全部</button></div>
        <button
          v-for="conversation in conversationHistory.slice(0, 2)"
          :key="conversation.id"
          class="recent-item"
          @click="selectConversation(conversation.id)"
        >
          <span>{{ conversationPreview(conversation.messages) }}</span>
          <time>{{ formatConversationTime(conversation.createdAt) }}</time>
        </button>
      </section>
    </div>

    <form class="ai-composer" @submit.prevent="sendMessage">
      <div class="composer-input-wrap">
        <div v-if="pendingImages.length || pendingTerminalSelection" class="image-attachments">
          <div v-if="pendingTerminalSelection" class="terminal-selection-attachment">
            <span>终端选区 · {{ pendingTerminalSelection.lineCount }} 行</span>
            <button type="button" class="remove-image-button" title="移除终端选区" aria-label="移除终端选区" @click="removePendingTerminalSelection"><X :size="10" /></button>
          </div>
          <div v-for="(image, index) in pendingImages" :key="image.dataUrl" class="image-attachment">
            <button type="button" class="image-thumbnail" :title="`预览 ${image.name}`" @click="openImagePreview(image)"><img :src="image.dataUrl" :alt="image.name" /></button>
            <button type="button" class="remove-image-button" title="移除图片" aria-label="移除图片" @click="removePendingImage(index)"><X :size="10" /></button>
          </div>
        </div>
        <textarea v-model="draft" :placeholder="`向 ${selectedAgent?.name ?? 'Agent'} 发送消息`" rows="3" @keydown.enter.exact.prevent="sendMessage" @paste="handlePaste" />
      </div>
      <footer class="composer-footer">
        <div class="composer-options">
          <button type="button" title="从剪贴板添加上下文" @click="pasteClipboardIntoDraft"><Plus :size="16" /></button>
          <button
            v-if="!activeModel"
            type="button"
            class="model-selector"
            title="配置 AI 模型"
            aria-label="配置 AI 模型"
            @click="emit('openAiSettings')"
          >
            <span class="model-dot">AI</span><span>{{ activeModelLabel }}</span>
          </button>
          <n-dropdown
            v-else
            v-model:show="showModelMenu"
            trigger="click"
            placement="top-start"
            :options="modelOptions"
            :disabled="(aiStore.config.models?.length ?? 0) < 2 || Boolean(requestId)"
          >
            <button type="button" class="model-selector" :title="activeModelLabel" aria-label="选择 AI 模型">
              <span class="model-dot">AI</span><span>{{ activeModelLabel }}</span><ChevronDown v-if="(aiStore.config.models?.length ?? 0) > 1" :size="12" />
            </button>
          </n-dropdown>
          <n-dropdown
            v-model:show="showExecutionModeMenu"
            trigger="click"
            placement="top-start"
            :options="executionModeOptions"
          >
            <button
              type="button"
              class="execution-mode"
              title="AI 执行模式"
              aria-label="AI 执行模式"
              :style="{ '--execution-mode-tone': currentExecutionMode.tone }"
            >
              <Command :size="13" />
              <span>{{ currentExecutionMode.label }}</span>
              <ChevronDown :size="12" />
            </button>
          </n-dropdown>
        </div>
        <button v-if="requestId" class="send-button stop-task-button" type="button" title="停止 AI 任务" aria-label="停止 AI 任务" @click="cancelTask"><X :size="17" /></button>
        <button v-else class="send-button" :disabled="!canSend" title="发送"><ArrowUp :size="17" /></button>
      </footer>
    </form>

    <div v-if="previewImage" class="image-preview-backdrop" role="dialog" aria-modal="true" :aria-label="`预览 ${previewImage.name}`" @click.self="previewImage = null">
      <div class="image-preview-dialog">
        <img :src="previewImage.dataUrl" :alt="previewImage.name" />
        <button type="button" title="关闭预览" aria-label="关闭预览" @click="previewImage = null"><X :size="18" /></button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.ai-chat-panel { display: flex; flex-direction: column; height: 100%; min-width: 0; background: #151a25; color: #c8d1e1; font-size: 12px; }
.ai-header { display: flex; align-items: center; justify-content: space-between; min-height: 47px; padding: 0 11px 0 14px; border-bottom: 1px solid #292f3d; }.agent-selector, .header-actions button, .section-heading button, .recent-item, .composer-options button, .expand-button, .send-button { border: 0; background: transparent; color: inherit; cursor: pointer; }.agent-selector { display: flex; align-items: center; gap: 7px; padding: 5px 2px; color: #c4d2e8; }.agent-selector svg:first-child { color: #b9c5d9; }.agent-selector strong { font-size: 12px; font-weight: 600; }.agent-selector svg:last-child { color: #647086; }.header-actions { display: flex; align-items: center; gap: 3px; }.header-actions button { display: grid; place-items: center; width: 25px; height: 25px; border-radius: 4px; color: #657185; }.header-actions button:hover { background: #252c3a; color: #b8c5d9; }.header-actions .close-button:hover { background: #a94a5c; color: #fff; }
.ai-body { position: relative; display: flex; flex: 1; flex-direction: column; min-height: 0; overflow: auto; }.ai-empty-state { display: grid; flex: 1; place-items: center; padding: 32px 22px 10px; }.ai-empty-state p { margin: 0; color: #647087; text-align: center; line-height: 1.6; }.ai-messages { display: flex; flex: 1; flex-direction: column; gap: 10px; padding: 16px; }.message { align-self: flex-end; max-width: 88%; }.message-content { padding: 8px 10px; border-radius: 7px; background: #2a3344; color: #d5deed; line-height: 1.55; overflow-wrap: anywhere; }.message-content :deep(p) { margin: 0 0 8px; }.message-content :deep(p:last-child) { margin-bottom: 0; }.message-content :deep(h1), .message-content :deep(h2), .message-content :deep(h3), .message-content :deep(h4) { margin: 13px 0 6px; color: #edf3ff; line-height: 1.35; }.message-content :deep(h1) { font-size: 16px; }.message-content :deep(h2) { font-size: 14px; }.message-content :deep(h3), .message-content :deep(h4) { font-size: 12px; }.message-content :deep(ul), .message-content :deep(ol) { margin: 6px 0; padding-left: 19px; }.message-content :deep(li + li) { margin-top: 3px; }.message-content :deep(code) { padding: 1px 4px; border-radius: 3px; background: #151b27; color: #b9d7ff; font-family: "Cascadia Code", "Fira Code", Consolas, monospace; font-size: .92em; }.message-content :deep(pre) { margin: 8px 0; padding: 9px 10px; overflow-x: auto; border: 1px solid #3a465c; border-radius: 5px; background: #111722; }.message-content :deep(pre code) { padding: 0; background: transparent; color: #d9e4f5; font-size: 11px; white-space: pre; }.message-content :deep(a) { color: #8db8ff; text-decoration: none; }.message-content :deep(a:hover) { text-decoration: underline; }.message-content :deep(blockquote) { margin: 7px 0; padding-left: 8px; border-left: 3px solid #6388c4; color: #afbed2; }.thinking-indicator { display: flex; align-items: center; align-self: flex-start; gap: 4px; padding: 3px 0; color: #8c9ab0; }.thinking-label { margin-left: 4px; font-size: 11px; }.service-hint { margin: 0; color: #e4b85d; font-size: 11px; text-align: center; }
.recent-section { padding: 0 16px 13px; }.section-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 7px; color: #667287; font-size: 11px; }.section-heading button { padding: 2px 0; color: #67758b; font-size: 11px; }.section-heading button:hover { color: #a9b8ce; }.recent-item { display: flex; align-items: center; justify-content: space-between; width: 100%; height: 31px; padding: 0; color: #aab9d1; text-align: left; }.recent-item span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.recent-item time { flex: 0 0 auto; padding-left: 10px; color: #5f6b80; font-size: 10px; }.recent-item:hover span { color: #d1dded; }
.ai-composer { flex: 0 0 auto; margin: 0 15px 15px; overflow: hidden; border: 1px solid #30394a; border-radius: 7px; background: #1a202c; box-shadow: 0 5px 18px rgba(0, 0, 0, .16); }.composer-input-wrap { position: relative; }.composer-input-wrap textarea { display: block; box-sizing: border-box; width: 100%; min-height: 63px; resize: none; padding: 11px 36px 7px 12px; border: 0; outline: none; background: transparent; color: #d7dfed; font: inherit; line-height: 1.5; }.composer-input-wrap textarea::placeholder { color: #66738a; }.expand-button { position: absolute; right: 8px; top: 8px; display: grid; place-items: center; width: 20px; height: 20px; color: #6e7a8e; }.expand-button:hover { color: #adbcd0; }.composer-footer { display: flex; align-items: flex-end; justify-content: space-between; min-height: 39px; padding: 0 7px 5px 9px; }.composer-options { display: flex; align-items: flex-end; height: 25px; min-width: 0; gap: 7px; }.composer-options > button { display: inline-flex; align-items: center; gap: 4px; height: 25px; padding: 0; color: #9aa9bf; font-size: 10px; white-space: nowrap; }.composer-options > button:hover { color: #d1dbea; }.model-selector { max-width: 160px; }.model-selector > span:not(.model-dot) { overflow: hidden; text-overflow: ellipsis; }.model-dot { display: grid; width: 16px; height: 16px; place-items: center; border-radius: 50%; background: #f98a1d; color: #fff; font-size: 7px; font-weight: 700; }.auto-mode { color: #9abef2 !important; }.auto-mode svg:first-child { color: #5bd8aa; }.send-button { display: grid; flex: 0 0 auto; place-items: center; width: 31px; height: 31px; border-radius: 50%; background: #323b4e; color: #c8d4e7; }.send-button:not(:disabled):hover { background: #78a6ff; color: #172033; }.send-button:disabled { cursor: default; opacity: .52; }
.model-selector { display: inline-flex; align-items: center; gap: 5px; width: min(160px, 100%); min-width: 0; height: 25px; padding: 0; color: #9aa9bf; font-size: 10px; white-space: nowrap; }.model-selector:not(:disabled):hover { color: #d1dbea; }.model-selector:disabled { cursor: default; }.model-selector > span:not(.model-dot) { min-width: 0; overflow: hidden; text-overflow: ellipsis; }.model-dot { flex: 0 0 16px; }

.test-connection-button { margin-top: 12px; padding: 7px 12px; border: 0; border-radius: 4px; background: #557fca; color: #fff; cursor: pointer; }.test-connection-button:disabled { cursor: default; opacity: .55; }.connection-test-result { margin-top: 8px !important; font-size: 11px; }.connection-test-result.success { color: #5bd8aa; }.connection-test-result.authentication_failed, .connection-test-result.model_unavailable, .connection-test-result.rate_limited, .connection-test-result.service_unavailable, .connection-test-result.timeout, .connection-test-result.network_error { color: #e4b85d; }

.message { position: relative; align-self: center; width: min(94%, 720px); max-width: none; }
.message .message-content { width: 100%; box-sizing: border-box; }
.message.assistant .message-content, .message.user .message-content { background: #2a3344; }
.message::before { position: absolute; top: 8px; z-index: 1; width: 12px; height: 12px; background: #2a3344; content: ''; transform: rotate(45deg); }
.message.assistant::before { left: -5px; }
.message.user::before { right: -5px; }
.message.user .message-content { white-space: pre-wrap; }
.message-images, .message-terminal-selections { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 7px; }
.terminal-selection-chip, .terminal-selection-attachment { display: inline-flex; align-items: center; min-height: 22px; border: 1px solid var(--app-border); border-radius: 4px; background: var(--app-code); color: var(--app-muted); font-size: 10px; }
.terminal-selection-chip { padding: 0 7px; }
.message-images img { max-width: min(240px, 100%); max-height: 180px; border-radius: 4px; object-fit: contain; }

.conversation-history { flex: 1; padding: 16px; overflow: auto; }
.history-empty { margin: 20px 0; color: #68758a; text-align: center; }
.history-item { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 38px; padding: 0 8px; border: 0; border-radius: 4px; background: transparent; color: #aab9d1; cursor: pointer; text-align: left; }
.history-item:hover, .history-item.active { background: #252e3e; color: #d7e1f2; }
.history-item span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.history-item time { flex: 0 0 auto; padding-left: 10px; color: #647188; font-size: 10px; }
.agent-menu-wrap { position: relative; }.agent-menu { position: absolute; z-index: 5; top: calc(100% + 5px); left: 0; display: flex; flex-direction: column; min-width: 190px; padding: 4px; border: 1px solid #344057; border-radius: 5px; background: #1c2330; box-shadow: 0 8px 18px rgba(0, 0, 0, .32); }.agent-menu button { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 28px; padding: 0 8px; border: 0; border-radius: 3px; background: transparent; color: #b9c6da; cursor: pointer; text-align: left; }.agent-menu button:hover, .agent-menu button.selected { background: #2a3548; color: #edf3ff; }.agent-menu small { color: #8091ad; font-size: 10px; }.agent-menu .agent-menu-action { justify-content: flex-start; border-top: 1px solid #303b4e; margin-top: 3px; padding-top: 4px; }.agent-editor { display: flex; flex: 1; flex-direction: column; gap: 9px; padding: 16px; }.agent-editor input, .agent-editor textarea { box-sizing: border-box; width: 100%; border: 1px solid #344057; border-radius: 4px; outline: none; background: #1a202c; color: #d7dfed; font: inherit; }.agent-editor input { height: 31px; padding: 0 9px; }.agent-editor textarea { min-height: 190px; padding: 9px; resize: vertical; line-height: 1.5; }.agent-editor p { margin: 0; color: #7f8ca2; line-height: 1.5; }.agent-editor-actions { display: flex; gap: 8px; }.agent-editor-actions button { display: inline-flex; align-items: center; gap: 5px; min-height: 29px; padding: 0 10px; border: 0; border-radius: 4px; background: #557fca; color: #fff; cursor: pointer; }.agent-editor-actions button:disabled { opacity: .55; cursor: default; }.agent-editor-actions .danger { background: #884253; }

.ai-plan, .ai-action-card { align-self: center; box-sizing: border-box; width: min(88%, 720px); border: 1px solid #36445d; border-radius: 6px; background: #1b2331; }
.ai-plan { padding: 9px 10px; color: #b9c7dc; }
.ai-plan strong { color: #dce7f7; font-size: 11px; }
.ai-plan p { margin: 5px 0 0; line-height: 1.5; white-space: pre-wrap; }
.ai-action-card { padding: 10px; border-color: #557fca; }
.ai-action-card.executing { border-color: #6087cc; }
.ai-action-card.awaiting_risk_confirmation, .ai-action-card.completed, .ai-action-card.failed, .ai-action-card.unconfirmed, .ai-action-card.terminal_blocked, .ai-action-card.recovery_failed, .ai-action-card.rejected { border-color: #292f3d; background: #151a25; transition: padding .24s ease, border-color .24s ease, background-color .24s ease; }
	.ai-action-card.awaiting_risk_confirmation.collapsed, .ai-action-card.completed.collapsed, .ai-action-card.failed.collapsed, .ai-action-card.unconfirmed.collapsed, .ai-action-card.terminal_blocked.collapsed, .ai-action-card.recovery_failed.collapsed, .ai-action-card.rejected.collapsed { padding: 0; }
.action-card-heading, .action-status, .action-card-meta { display: flex; align-items: center; }
.action-card-heading { justify-content: space-between; gap: 8px; color: #e5edf9; }
.action-status { min-width: 0; gap: 6px; }
.action-status svg { flex: 0 0 auto; color: #8fb2ee; }
.ai-action-card.completed .action-status { color: #c8d1e1; }
	.ai-action-card.completed .action-status svg { color: #67b99a; }
	.ai-action-card.awaiting_risk_confirmation .action-status svg, .ai-action-card.failed .action-status svg, .ai-action-card.unconfirmed .action-status svg, .ai-action-card.terminal_blocked .action-status svg, .ai-action-card.recovery_failed .action-status svg, .ai-action-card.rejected .action-status svg { color: #e1a66e; }
.action-card-meta { flex: 0 0 auto; gap: 3px; }
.action-card-heading span { color: #9aa8be; font-size: 10px; }
.ai-action-card.completed .action-card-heading span { color: #9aa8be; }
	.action-collapse-button { display: grid; place-items: center; width: 22px; height: 22px; padding: 0; border: 0; border-radius: 3px; background: transparent; color: #9aa8be; cursor: pointer; }
	.action-collapse-button:hover { background: #252c3a; color: #d5deed; }
.action-collapse-button svg { transition: transform .16s ease; }
.action-details { display: grid; grid-template-rows: 1fr; overflow: hidden; }
.action-details-inner { min-height: 0; }
.action-details-enter-active, .action-details-leave-active { transition: grid-template-rows .24s ease, opacity .18s ease; }
.action-details-enter-from, .action-details-leave-to { grid-template-rows: 0fr; opacity: 0; }
.completed-action-row { display: grid; grid-template-columns: auto auto minmax(0, 1fr) auto; align-items: center; width: 100%; min-height: 36px; gap: 7px; padding: 0 9px; border: 0; background: transparent; color: #9aa8be; cursor: pointer; font: inherit; text-align: left; }
	.completed-action-row:hover { background: #202633; color: #d5deed; }
.completed-action-row > svg:first-child { transition: transform .24s ease; }
.completed-action-row > svg.collapsed { transform: rotate(-90deg); }
.completed-action-row > svg:nth-child(2) { color: #61ad91; }
	.completed-action-row.awaiting_risk_confirmation > svg:nth-child(2), .completed-action-row.failed > svg:nth-child(2), .completed-action-row.unconfirmed > svg:nth-child(2), .completed-action-row.terminal_blocked > svg:nth-child(2), .completed-action-row.recovery_failed > svg:nth-child(2), .completed-action-row.rejected > svg:nth-child(2) { color: #d28e62; }
	.completed-action-row code { overflow: hidden; color: #b9c7dc; font-family: "Cascadia Code", "Fira Code", Consolas, monospace; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
	.completed-action-row span { color: #7f8ca2; font-size: 10px; white-space: nowrap; }
.status-spinner { animation: action-spin .9s linear infinite; }
@keyframes action-spin { to { transform: rotate(360deg); } }
.action-output { max-height: 150px; margin-top: 8px; overflow-y: auto; padding-right: 4px; }
.action-plan { margin: 0 0 8px; color: #b9c7dc; line-height: 1.45; white-space: pre-wrap; }
.action-output > code { display: block; padding: 7px 8px; overflow-x: auto; border-radius: 4px; background: #111722; color: #b9d7ff; font-family: "Cascadia Code", "Fira Code", Consolas, monospace; font-size: 11px; white-space: pre-wrap; }
.ai-action-card dl { margin: 9px 0; }
.ai-action-card dl div { display: grid; grid-template-columns: 32px minmax(0, 1fr); gap: 7px; line-height: 1.45; }
.ai-action-card dt { color: #8493aa; }
.ai-action-card dd { margin: 0; color: #c6d0df; overflow-wrap: anywhere; }
.executable-authorizations { display: flex; flex-direction: column; gap: 8px; margin: 10px 0; }
.executable-authorization { display: grid; grid-template-columns: 72px minmax(0, 1fr); align-items: center; gap: 7px; }
.executable-authorization > code { overflow: hidden; color: #b9d7ff; font-family: "Cascadia Code", "Fira Code", Consolas, monospace; text-overflow: ellipsis; white-space: nowrap; }
.grant-options { display: flex; flex-wrap: wrap; gap: 4px; }
.grant-options button, .action-card-buttons button { min-height: 27px; padding: 0 8px; border: 1px solid #495773; border-radius: 4px; background: #283246; color: #d4deed; cursor: pointer; font: inherit; }
.grant-options button { font-size: 10px; }
.grant-options button.selected { border-color: #5d88d6; background: #405e95; color: #fff; }
.grant-options button.reject.selected { border-color: #b76b76; background: #6e3f4b; }
.action-card-buttons { display: flex; justify-content: flex-end; gap: 7px; }
.authorization-actions { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
.bulk-grant-select { display: flex; align-items: center; min-width: 0; gap: 6px; color: #9aa8be; font-size: 10px; white-space: nowrap; }
.bulk-grant-select select { min-width: 0; height: 27px; padding: 0 22px 0 8px; border: 1px solid #495773; border-radius: 4px; outline: none; background: #283246; color: #d4deed; font: inherit; font-size: 10px; cursor: pointer; }
.bulk-grant-select select:hover:not(:disabled), .bulk-grant-select select:focus-visible { border-color: #5d88d6; background: #344864; }
.bulk-grant-select select:disabled { cursor: default; opacity: .55; }
.action-card-buttons button.approve { white-space: nowrap; border-color: #5d88d6; background: #557fca; color: #fff; }
.action-card-buttons button:disabled { cursor: default; opacity: .55; }
.action-progress { display: flex; align-items: center; gap: 6px; margin-top: 9px; color: #9eb4d4; font-size: 11px; }
.action-progress svg { color: #8fb2ee; }
.action-result { max-height: 8.7em; margin: 9px 0 0; overflow-y: auto; padding: 8px 4px 0 0; border-top: 1px solid #334054; color: #9ddbc4; line-height: 1.45; white-space: pre-wrap; overflow-wrap: anywhere; }
.ai-action-card.awaiting_risk_confirmation .action-result, .ai-action-card.failed .action-result, .ai-action-card.unconfirmed .action-result, .ai-action-card.terminal_blocked .action-result, .ai-action-card.recovery_failed .action-result, .ai-action-card.rejected .action-result { color: #e4c080; }
.ai-body, .action-output, .action-result, .message-content :deep(pre), .action-output > code { scrollbar-width: thin; scrollbar-color: #55627b transparent; }
.ai-body::-webkit-scrollbar, .action-output::-webkit-scrollbar, .action-result::-webkit-scrollbar, .message-content :deep(pre::-webkit-scrollbar), .action-output > code::-webkit-scrollbar { width: 6px; height: 6px; }
.ai-body::-webkit-scrollbar-track, .action-output::-webkit-scrollbar-track, .action-result::-webkit-scrollbar-track, .message-content :deep(pre::-webkit-scrollbar-track), .action-output > code::-webkit-scrollbar-track { background: transparent; }
.ai-body::-webkit-scrollbar-thumb, .action-output::-webkit-scrollbar-thumb, .action-result::-webkit-scrollbar-thumb, .message-content :deep(pre::-webkit-scrollbar-thumb), .action-output > code::-webkit-scrollbar-thumb { border-radius: 3px; background: #55627b; }
.ai-body::-webkit-scrollbar-thumb:hover, .action-output::-webkit-scrollbar-thumb:hover, .action-result::-webkit-scrollbar-thumb:hover, .message-content :deep(pre::-webkit-scrollbar-thumb:hover), .action-output > code::-webkit-scrollbar-thumb:hover { background: #71809c; }
.execution-mode { display: inline-flex; align-items: center; gap: 4px; height: 25px; padding: 0 5px; border: 1px solid #3a465d; border-radius: 4px; background: #202938; color: #c7d5e9; cursor: pointer; font: inherit; white-space: nowrap; }
.execution-mode svg:first-child { color: var(--execution-mode-tone); }
.execution-mode svg:last-child { color: #70809a; pointer-events: none; }
.execution-mode:hover { border-color: #5875a8; color: #e2ebf8; }

.thinking-indicator { display: flex; align-items: center; align-self: flex-start; gap: 5px; padding: 3px 0; color: #8c9ab0; }
.thinking-squares { display: block; flex: 0 0 12px; width: 15px; height: 15px; overflow: hidden; transform: scale(.8); transform-origin: left center; }
.thinking-squares rect { fill: #7d9fdb; }
.thinking-label { margin: 0; font-size: 11px; }
.task-error-card { align-self: center; box-sizing: border-box; width: min(88%, 720px); padding: 10px; border: 1px solid #87634d; border-radius: 6px; background: #261f1b; color: #e4c080; }
.task-error-card > div { display: flex; align-items: center; gap: 6px; color: #f0c58a; }
.task-error-card svg { flex: 0 0 auto; }
.task-error-card p { margin: 6px 0 0 22px; line-height: 1.5; }
.task-inline-action { margin: 0; padding: 0; border: 0; background: transparent; color: #8db8ff; cursor: pointer; font: inherit; text-decoration: underline; text-underline-offset: 2px; }
.task-inline-action:hover { color: #b9d7ff; }
	.task-inline-action:focus-visible { outline: 1px solid #8db8ff; outline-offset: 2px; }
	.stop-task-button { background: #663d4a; color: #ffd4dc; }
	.stop-task-button:hover { background: #a94a5c !important; color: #fff !important; }

.ai-chat-panel { background: var(--app-panel); color: var(--app-text); }
.ai-header { border-bottom-color: var(--app-border); }
.agent-selector { color: var(--app-text); }
.agent-selector svg:first-child, .header-actions button, .agent-selector svg:last-child { color: var(--app-muted); }
.header-actions button:hover, .action-collapse-button:hover { background: var(--app-hover); color: var(--app-text); }
.ai-empty-state p, .thinking-indicator, .thinking-label, .service-hint, .section-heading, .section-heading button, .recent-item time { color: var(--app-muted); }
.message-content, .message.assistant .message-content, .message.user .message-content, .message::before { background: var(--app-hover); color: var(--app-text); }
.message-content :deep(h1), .message-content :deep(h2), .message-content :deep(h3), .message-content :deep(h4) { color: var(--app-text); }
.message-content :deep(code), .action-output > code { background: var(--app-code); color: var(--app-text); }
.message-content :deep(pre) { border-color: var(--app-border); background: var(--app-code); }
.message-content :deep(pre code) { color: var(--app-text); }
.recent-item { color: var(--app-text); }
.recent-item:hover span, .history-item:hover, .history-item.active { color: var(--app-text); }
.history-item:hover, .history-item.active, .completed-action-row:hover { background: var(--app-hover); }
.ai-composer, .agent-editor input, .agent-editor textarea { border-color: var(--app-border); background: var(--app-surface); color: var(--app-text); box-shadow: 0 5px 18px var(--app-shadow); }
.composer-input-wrap textarea { color: var(--app-text); }
.composer-input-wrap textarea::placeholder { color: var(--app-muted); }
.composer-options > button, .model-selector, .execution-mode, .bulk-grant-select { color: var(--app-muted); }
.send-button { background: var(--app-hover); color: var(--app-text); }
.ai-plan, .ai-action-card { border-color: var(--app-border); background: var(--app-surface); color: var(--app-text); }
.ai-action-card.awaiting_risk_confirmation, .ai-action-card.completed, .ai-action-card.failed, .ai-action-card.unconfirmed, .ai-action-card.terminal_blocked, .ai-action-card.recovery_failed, .ai-action-card.rejected { border-color: var(--app-border); background: var(--app-panel); }
.action-card-heading, .ai-plan strong, .action-plan, .ai-action-card dd, .completed-action-row code { color: var(--app-text); }
.action-card-heading span, .action-card-meta, .ai-action-card dt, .completed-action-row, .completed-action-row span, .bulk-grant-select { color: var(--app-muted); }
.grant-options button, .action-card-buttons button, .bulk-grant-select select, .execution-mode { border-color: var(--app-border); background: var(--app-hover); color: var(--app-text); }
.grant-options button.selected { border-color: var(--app-accent); background: var(--app-selection); color: var(--app-text); }
.action-result { border-top-color: var(--app-border); }
.agent-menu { border-color: var(--app-border); background: var(--app-panel); box-shadow: 0 8px 18px var(--app-shadow); }
.agent-menu button { color: var(--app-text); }
.agent-menu button:hover, .agent-menu button.selected { background: var(--app-hover); color: var(--app-text); }
.execution-mode { border-color: var(--app-border); background: var(--app-hover); color: var(--app-text); }
.execution-mode svg:last-child { color: var(--app-muted); }
.task-error-card { border-color: #b45309; background: color-mix(in srgb, #f59e0b 12%, var(--app-panel)); }
.image-attachments { display: flex; max-width: 100%; gap: 5px; overflow-x: auto; padding: 7px 10px 0; }
.image-attachment { position: relative; flex: 0 0 32px; width: 32px; height: 32px; }
.terminal-selection-attachment { position: relative; flex: 0 0 auto; min-height: 32px; padding: 0 24px 0 8px; }
.image-thumbnail { display: block; width: 32px; height: 32px; padding: 0; overflow: hidden; border: 1px solid var(--app-border); border-radius: 4px; background: var(--app-code); cursor: pointer; }
.image-thumbnail:hover { border-color: var(--app-accent); }
.image-thumbnail img { display: block; width: 100%; height: 100%; object-fit: cover; }
.remove-image-button { position: absolute; top: -5px; right: -5px; display: grid; place-items: center; width: 15px; height: 15px; padding: 0; border: 1px solid var(--app-border); border-radius: 50%; background: var(--app-panel); color: var(--app-muted); cursor: pointer; }
.remove-image-button:hover { border-color: var(--app-accent); color: var(--app-text); }
.image-preview-backdrop { position: fixed; z-index: 1000; inset: 0; display: grid; place-items: center; padding: 24px; background: rgba(0, 0, 0, .68); }
.image-preview-dialog { position: relative; display: grid; max-width: min(900px, 100%); max-height: 100%; padding: 10px; border: 1px solid var(--app-border); border-radius: 6px; background: var(--app-panel); box-shadow: 0 16px 48px var(--app-shadow); }
.image-preview-dialog img { max-width: 100%; max-height: calc(100vh - 68px); object-fit: contain; }
.image-preview-dialog button { position: absolute; top: 16px; right: 16px; display: grid; place-items: center; width: 28px; height: 28px; padding: 0; border: 1px solid var(--app-border); border-radius: 4px; background: var(--app-panel); color: var(--app-text); cursor: pointer; }
.image-preview-dialog button:hover { background: var(--app-hover); }
</style>

<style>
.model-option {
  display: flex;
  width: calc(100% - 8px);
  min-width: 190px;
  flex-direction: column;
  gap: 2px;
  margin: 0 4px;
  padding: 7px 8px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--n-text-color);
  cursor: pointer;
  font: inherit;
  text-align: left;
}

.model-option:hover, .model-option.selected {
  background: var(--n-option-color-pending);
}

.model-option-name {
  color: var(--n-text-color);
  font-size: 11px;
  line-height: 1.35;
}

.model-option-meta {
  color: var(--n-text-color-3);
  font-size: 9px;
  line-height: 1.35;
}

.execution-mode-option {
  display: block;
  width: calc(100% - 8px);
  min-width: 178px;
  margin: 0 4px;
  padding: 6px 8px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  font: inherit;
  text-align: left;
}

.execution-mode-option:hover {
  background: var(--n-option-color-pending);
}

.execution-mode-option.selected .execution-mode-option-title {
  color: var(--n-text-color);
}

.execution-mode-option-copy {
  display: grid;
  min-width: 0;
}

.execution-mode-option-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.execution-mode-option-title {
  color: var(--n-text-color);
  font-size: 11px;
  line-height: 1.35;
}

.execution-mode-option-risk {
  flex: 0 0 auto;
  font-size: 10px;
  line-height: 1.35;
}

.execution-mode-option-description {
  overflow: hidden;
  color: var(--n-text-color-3);
  font-size: 9px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
