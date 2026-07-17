export type AiExecutionMode = 'read_only' | 'approval_required' | 'autonomous'

export interface AiProviderConfigView {
  configured: boolean
  providerType: 'openai_compatible'
  baseUrl: string | null
  model: string | null
  timeoutSeconds: number | null
}

export type AiConnectionTestStatus =
  | 'success'
  | 'authentication_failed'
  | 'model_unavailable'
  | 'rate_limited'
  | 'service_unavailable'
  | 'timeout'
  | 'network_error'

export interface AiConnectionTestResult {
  status: AiConnectionTestStatus
  message: string
}

export interface SaveAiProviderConfigRequest {
  baseUrl: string
  apiKey: string
  model: string
  timeoutSeconds: number
}

export interface AiChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export interface AiAgentConfig {
  id: string
  name: string
  prompt: string
  isDefault: boolean
  createdAt: string
  updatedAt: string
}

export interface SaveAiAgentConfigRequest {
  id?: string
  name: string
  prompt: string
}

export interface StartAiTaskInput {
  requestId: string
  sessionId: string
  conversationId: string
  agentId?: string
  messages: AiChatMessage[]
  executionMode: AiExecutionMode
  scopes: string[]
  includeTerminalContext: boolean
  terminalContext?: string
}

export interface AiTaskStarted {
  requestId: string
}

export type AiStreamEventType =
  | 'delta'
  | 'plan'
  | 'action_pending'
  | 'risk_confirmation_required'
  | 'action_started'
  | 'action_completed'
  | 'task_status'
  | 'policy_rejected'
  | 'completed'
  | 'cancelled'
  | 'error'

export interface AiPendingAction {
  actionId: string
  command: string
  purpose: string
  expectedImpact: string
  riskLevel: string
  rollbackHint: string
  missingExecutables: string[]
  riskConfirmationId?: string
  riskReason?: string
}

export interface AiActionResult {
  actionId: string
  status: 'completed' | 'failed' | 'unconfirmed' | 'terminal_blocked' | 'recovery_failed' | 'rejected' | 'awaiting_risk_confirmation'
  summary: string
}

export interface AiStreamEvent {
  requestId: string
  eventType: AiStreamEventType
  content?: string
  action?: AiPendingAction
  actionResult?: AiActionResult
}

export type AiExecutableGrant = 'once' | 'server' | 'global' | 'reject'

export interface AiExecutableDecision {
  executable: string
  grant: AiExecutableGrant
}

export interface AiActionDecisionInput {
  requestId: string
  actionId: string
  decisions: AiExecutableDecision[]
}
