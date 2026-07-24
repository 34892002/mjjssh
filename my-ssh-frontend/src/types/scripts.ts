export type ScriptRiskLevel = 'low' | 'medium' | 'high'

export interface ScriptView {
  id: string
  name: string
  description: string | null
  tags: string[]
  command: string
  riskLevel: ScriptRiskLevel
  createdAt: string
  updatedAt: string
}

export interface CreateScriptRequest {
  name: string
  description?: string | null
  tags?: string[]
  command: string
  riskLevel: ScriptRiskLevel
}

export type UpdateScriptRequest = CreateScriptRequest

export interface ScriptSubscriptionView {
  id: string
  name: string
  url: string
  enabled: boolean
  etag: string | null
  lastFetchedAt: string | null
  lastSuccessAt: string | null
  lastError: string | null
}

export interface CreateScriptSubscriptionRequest {
  name: string
  url: string
}

export interface UpdateScriptSubscriptionRequest {
  name?: string
  url?: string
  enabled?: boolean
}

export interface SubscriptionScriptView {
  subscriptionId: string
  id: string
  name: string
  description: string | null
  tags: string[]
  command: string
  riskLevel: ScriptRiskLevel
  platforms: string[]
  version: string
  homepage: string | null
}
