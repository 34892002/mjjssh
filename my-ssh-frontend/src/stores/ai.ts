import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type {
  AiActionResult,
  AiAgentConfig,
  AiConnectionTestResult,
  AiPendingAction,
  AiProviderConfigView,
  SaveAiAgentConfigRequest,
  SaveAiProviderConfigRequest,
} from '../types/ai'

export interface AiChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
}

export interface AiActionRecord {
  messageId: string
  action: AiPendingAction
  plan?: string
  status: 'awaiting_authorization' | 'awaiting_risk_confirmation' | 'executing' | AiActionResult['status']
  createdAt: number
  startedAt?: number
  finishedAt?: number
  collapsed?: boolean
  phase?: string
  result?: AiActionResult
}

export interface AiConversation {
  id: string
  createdAt: number
  messages: AiChatMessage[]
  actionHistory: AiActionRecord[]
}

interface AiSessionConversations {
  activeConversationId: string
  conversations: AiConversation[]
}

function createConversation(): AiConversation {
  return {
    id: crypto.randomUUID(),
    createdAt: Date.now(),
    messages: [],
    actionHistory: [],
  }
}

const unconfigured: AiProviderConfigView = {
  configured: false,
  providerType: 'openai_compatible',
  baseUrl: null,
  model: null,
  timeoutSeconds: null,
}

export const useAiStore = defineStore('ai', {
  state: () => ({
    config: unconfigured as AiProviderConfigView,
    loading: false,
    error: null as string | null,
    agents: [] as AiAgentConfig[],
    selectedAgentId: null as string | null,
    sessionConversations: {} as Record<string, AiSessionConversations>,
  }),
  actions: {
    getSessionConversations(sessionId: string) {
      const existing = this.sessionConversations[sessionId]
      if (existing) return existing

      const conversation = createConversation()
      const sessionConversations = {
        activeConversationId: conversation.id,
        conversations: [conversation],
      }
      this.sessionConversations[sessionId] = sessionConversations
      return sessionConversations
    },
    getConversation(sessionId: string) {
      const sessionConversations = this.getSessionConversations(sessionId)
      return sessionConversations.conversations.find(
        (conversation) => conversation.id === sessionConversations.activeConversationId,
      )?.messages ?? []
    },
    setConversation(sessionId: string, messages: AiChatMessage[]) {
      const sessionConversations = this.getSessionConversations(sessionId)
      const conversation = sessionConversations.conversations.find(
        (item) => item.id === sessionConversations.activeConversationId,
      )
      if (conversation) conversation.messages = messages
    },
    getActionHistory(sessionId: string) {
      const sessionConversations = this.getSessionConversations(sessionId)
      return sessionConversations.conversations.find(
        (conversation) => conversation.id === sessionConversations.activeConversationId,
      )?.actionHistory ?? []
    },
    setActionHistory(sessionId: string, actionHistory: AiActionRecord[]) {
      const sessionConversations = this.getSessionConversations(sessionId)
      const conversation = sessionConversations.conversations.find(
        (item) => item.id === sessionConversations.activeConversationId,
      )
      if (conversation) conversation.actionHistory = actionHistory
    },
    getConversationHistory(sessionId: string) {
      return this.getSessionConversations(sessionId).conversations
        .filter((conversation) => conversation.messages.length > 0)
        .sort((left, right) => right.createdAt - left.createdAt)
    },
    startNewConversation(sessionId: string) {
      const sessionConversations = this.getSessionConversations(sessionId)
      const activeConversation = sessionConversations.conversations.find(
        (conversation) => conversation.id === sessionConversations.activeConversationId,
      )
      if (!activeConversation?.messages.length) return

      const conversation = createConversation()
      sessionConversations.conversations.unshift(conversation)
      sessionConversations.activeConversationId = conversation.id
    },
    selectConversation(sessionId: string, conversationId: string) {
      const sessionConversations = this.getSessionConversations(sessionId)
      if (sessionConversations.conversations.some((conversation) => conversation.id === conversationId)) {
        sessionConversations.activeConversationId = conversationId
      }
    },
    async loadAgents() {
      this.error = null
      try {
        this.agents = await invoke<AiAgentConfig[]>('list_ai_agents')
        if (!this.agents.some((agent) => agent.id === this.selectedAgentId)) {
          this.selectedAgentId = this.agents.find((agent) => agent.isDefault)?.id ?? this.agents[0]?.id ?? null
        }
      } catch (error) {
        this.error = String(error)
      }
    },
    selectAgent(agentId: string) {
      if (this.agents.some((agent) => agent.id === agentId)) this.selectedAgentId = agentId
    },
    async saveAgent(agent: SaveAiAgentConfigRequest) {
      this.error = null
      try {
        const saved = await invoke<AiAgentConfig>('save_ai_agent', { agent })
        const index = this.agents.findIndex((item) => item.id === saved.id)
        if (index >= 0) this.agents.splice(index, 1, saved)
        else this.agents.push(saved)
        this.selectedAgentId = saved.id
        return saved
      } catch (error) {
        this.error = String(error)
        throw error
      }
    },
    async deleteAgent(agentId: string) {
      this.error = null
      try {
        await invoke('delete_ai_agent', { id: agentId })
        this.agents = this.agents.filter((agent) => agent.id !== agentId)
        if (this.selectedAgentId === agentId) {
          this.selectedAgentId = this.agents.find((agent) => agent.isDefault)?.id ?? this.agents[0]?.id ?? null
        }
      } catch (error) {
        this.error = String(error)
        throw error
      }
    },
    async loadConfigStatus() {
      this.loading = true
      this.error = null
      try {
        this.config = await invoke<AiProviderConfigView>('get_ai_config_status')
      } catch (error) {
        this.error = String(error)
      } finally {
        this.loading = false
      }
    },
    async saveConfig(config: SaveAiProviderConfigRequest) {
      this.loading = true
      this.error = null
      try {
        await invoke('save_ai_config', { config })
        await this.loadConfigStatus()
      } catch (error) {
        this.error = String(error)
        throw error
      } finally {
        this.loading = false
      }
    },
    async testConnection() {
      this.error = null
      try {
        return await invoke<AiConnectionTestResult>('test_ai_connection')
      } catch (error) {
        this.error = String(error)
        throw error
      }
    },
    async deleteConfig() {
      this.loading = true
      this.error = null
      try {
        await invoke('delete_ai_config')
        this.config = { ...unconfigured }
      } catch (error) {
        this.error = String(error)
        throw error
      } finally {
        this.loading = false
      }
    },
  },
})
