<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NAlert,
  NButton,
  NButtonGroup,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NModal,
  useMessage,
  useThemeVars,
  NPopconfirm,
  NSelect,
  NSwitch,
  NTabPane,
  NTabs,
} from 'naive-ui'
import { Plus, RefreshCw, Sparkles } from '@lucide/vue'
import { useAiStore } from '../stores/ai'
import type { AiAgentConfig, AiConnectionTestResult, AiModelConfig } from '../types/ai'

const aiStore = useAiStore()
const message = useMessage()
const themeVars = useThemeVars()
const modalThemeStyle = computed(() => ({
  '--app-base': themeVars.value.baseColor,
  '--app-surface': themeVars.value.modalColor,
  '--app-elevated': themeVars.value.hoverColor,
  '--app-border': themeVars.value.borderColor,
  '--app-text': themeVars.value.textColor1,
  '--app-muted': themeVars.value.textColor3,
  '--app-hover': themeVars.value.hoverColor,
  '--app-accent': themeVars.value.primaryColor,
  '--app-panel': themeVars.value.modalColor,
  '--app-code': themeVars.value.codeColor,
  '--app-selection': themeVars.value.pressedColor,
  '--app-shadow': themeVars.value.boxShadow2,
}))
const discoveredModelsModalStyle = computed(() => ({
  ...modalThemeStyle.value,
  width: 'min(520px, calc(100vw - 32px))',
}))
const modelEditorModalStyle = computed(() => ({
  ...modalThemeStyle.value,
  width: 'min(680px, calc(100vw - 32px))',
}))
const activeTab = ref('provider')
const configForm = ref({ baseUrl: '', apiKey: '', model: '', models: [] as AiModelConfig[], activeModelId: null as string | null, timeoutSeconds: 60 })
const editingModel = ref<AiModelConfig | null>(null)
const showModelEditor = ref(false)
const modelForm = ref<AiModelConfig>(createModel())
const savingConfig = ref(false)
const testingConnection = ref(false)
const discoveringModels = ref(false)
const showDiscoveredModels = ref(false)
const discoveredModelIds = ref<string[]>([])
const pendingDiscoveredModelIds = ref<string[]>([])
const connectionTest = ref<AiConnectionTestResult | null>(null)
const editingAgent = ref<AiAgentConfig | null>(null)
const showAgentEditor = ref(false)
const agentForm = ref({ name: '', prompt: '' })
const savingAgent = ref(false)
const configured = computed(() => aiStore.config.configured)
const providerOptions = [{ label: 'OpenAI-compatible', value: 'openai_compatible' }]
const activeModel = computed(() => configForm.value.models.find((model) => model.id === configForm.value.activeModelId) ?? null)

const discoveredModelOptions = computed(() => discoveredModelIds.value
  .filter((id) => !configForm.value.models.some((model) => model.id === id))
  .map((id) => ({ label: id, value: id })))

function createModel(): AiModelConfig {
  return {
    id: '',
    name: '',
    maxContextTokens: 128000,
    maxOutputTokens: 16384,
    supportsTools: true,
    supportsImages: false,
    supportsParallelToolCalls: false,
    supportsPromptCaching: false,
    supportsReasoning: false,
    protocol: 'chat_completions',
    reasoningEffort: null,
    promptCacheKey: null,
  }
}

function syncConfigForm() {
  configForm.value.baseUrl = aiStore.config.baseUrl ?? ''
  configForm.value.models = aiStore.config.models?.map((model) => ({ ...model })) ?? []
  configForm.value.activeModelId = aiStore.config.activeModelId ?? configForm.value.models[0]?.id ?? null
  configForm.value.model = configForm.value.models.find((model) => model.id === configForm.value.activeModelId)?.name ?? ''
  configForm.value.timeoutSeconds = aiStore.config.timeoutSeconds ?? 60
  configForm.value.apiKey = ''
}

function selectModel(modelId: string | null) {
  const model = configForm.value.models.find((item) => item.id === modelId)
  configForm.value.activeModelId = model?.id ?? null
  configForm.value.model = model?.name ?? ''
}

function closeDiscoveredModels() {
  pendingDiscoveredModelIds.value = []
  showDiscoveredModels.value = false
}

function insertDiscoveredModels() {
  const addedModels = pendingDiscoveredModelIds.value.map((id) => ({ ...createModel(), id, name: id }))
  configForm.value.models.push(...addedModels)
  if (!configForm.value.activeModelId && addedModels[0]) selectModel(addedModels[0].id)
  closeDiscoveredModels()
}

async function discoverModels() {
  if (discoveringModels.value) return
  if (!configForm.value.baseUrl.trim()) {
    aiStore.error = '请输入 API 地址'
    return
  }
  if (!configForm.value.apiKey.trim() && !configured.value) {
    aiStore.error = '请输入 API Key'
    return
  }

  discoveringModels.value = true
  aiStore.error = null
  try {
    const models = await aiStore.discoverModels({
      baseUrl: configForm.value.baseUrl,
      apiKey: configForm.value.apiKey,
      timeoutSeconds: configForm.value.timeoutSeconds,
    })
    discoveredModelIds.value = models
    if (!models.length) {
      message.warning('该 API 未返回可用模型')
      return
    }
    pendingDiscoveredModelIds.value = []
    showDiscoveredModels.value = true
  } finally {
    discoveringModels.value = false
  }
}

function openNewModel() {
  editingModel.value = null
  modelForm.value = createModel()
  showModelEditor.value = true
}

function openEditModel(model: AiModelConfig) {
  editingModel.value = model
  modelForm.value = { ...model }
  showModelEditor.value = true
}

function closeModelEditor() {
  editingModel.value = null
  modelForm.value = createModel()
  showModelEditor.value = false
}

function handleModelEditorVisibility(show: boolean) {
  if (!show) closeModelEditor()
}

function saveModel() {
  const model = { ...modelForm.value, id: modelForm.value.id.trim(), name: modelForm.value.name.trim() }
  if (!model.id || !model.name) return
  const duplicate = configForm.value.models.some((item) =>
    item.id === model.id && item.id !== editingModel.value?.id ||
    item.name === model.name && item.id !== editingModel.value?.id,
  )
  if (duplicate) {
    message.error('模型 ID 和显示名称不能重复')
    return
  }

  const index = editingModel.value
    ? configForm.value.models.findIndex((item) => item.id === editingModel.value?.id)
    : -1
  if (index >= 0) configForm.value.models.splice(index, 1, model)
  else configForm.value.models.push(model)
  if (!configForm.value.activeModelId || editingModel.value?.id === configForm.value.activeModelId) selectModel(model.id)
  closeModelEditor()
}

function removeModel(model: AiModelConfig) {
  if (model.id === configForm.value.activeModelId) {
    message.warning('请先设置其他默认模型')
    return
  }
  const index = configForm.value.models.findIndex((item) => item.id === model.id)
  if (index < 0) return
  configForm.value.models.splice(index, 1)
}

onMounted(async () => {
  await Promise.all([aiStore.loadConfigStatus(), aiStore.loadAgents()])
  syncConfigForm()
})

async function saveConfig() {
  if (!configured.value && !configForm.value.apiKey.trim()) {
    aiStore.error = '首次保存配置时请输入 API Key'
    return
  }

  if (!activeModel.value) {
    aiStore.error = '请添加并选择一个模型'
    return
  }

  configForm.value.model = activeModel.value.name
  savingConfig.value = true
  connectionTest.value = null
  try {
    await aiStore.saveConfig(configForm.value)
    syncConfigForm()
    message.success('配置已保存')
  } finally {
    savingConfig.value = false
  }
}

async function testConnection() {
  if (!configured.value || testingConnection.value) return
  testingConnection.value = true
  connectionTest.value = null
  try {
    connectionTest.value = await aiStore.testConnection(activeModel.value?.id)
  } finally {
    testingConnection.value = false
  }
}

function openNewAgent() {
  editingAgent.value = null
  agentForm.value = { name: '', prompt: '' }
  showAgentEditor.value = true
}

function openEditAgent(agent: AiAgentConfig) {
  editingAgent.value = agent
  agentForm.value = { name: agent.name, prompt: agent.prompt }
  showAgentEditor.value = true
}

function closeAgentEditor() {
  editingAgent.value = null
  agentForm.value = { name: '', prompt: '' }
  showAgentEditor.value = false
}

async function saveAgent() {
  savingAgent.value = true
  try {
    await aiStore.saveAgent({ id: editingAgent.value?.id, ...agentForm.value })
    closeAgentEditor()
  } finally {
    savingAgent.value = false
  }
}

async function deleteAgent(agent: AiAgentConfig) {
  await aiStore.deleteAgent(agent.id)
  if (editingAgent.value?.id === agent.id) closeAgentEditor()
}
</script>

<template>
  <section class="ai-settings">
    <n-alert v-if="aiStore.error" type="error" :show-icon="false" class="settings-alert">{{ aiStore.error }}</n-alert>

    <n-tabs v-model:value="activeTab" type="line" class="ai-tabs" animated>
      <n-tab-pane name="provider" tab="提供商">
        <section class="tab-section">
          <h3>AI 提供商</h3>
          <p class="section-description">配置用于 AI 对话的 OpenAI-compatible 服务。</p>

          <div class="settings-card">
            <div class="settings-row">
              <div class="row-description"><strong>提供商</strong><p>当前支持标准 OpenAI-compatible API。</p></div>
              <n-select :value="'openai_compatible'" :options="providerOptions" disabled class="row-control" />
            </div>
            <div class="settings-row">
              <div class="row-description"><strong>API 密钥</strong><p>用于访问服务的密钥，保存后不会再次显示。</p></div>
              <n-input v-model:value="configForm.apiKey" type="password" show-password-on="click" autocomplete="off" :placeholder="configured ? '输入 API Key 以更新配置' : '输入 API Key…'" class="row-control" />
            </div>
            <div class="settings-row">
              <div class="row-description"><strong>API 地址</strong><p>自定义 API 端点，保持使用 `/v1` 前缀。</p></div>
              <n-input v-model:value="configForm.baseUrl" placeholder="https://api.openai.com/v1" class="row-control" />
            </div>

            <div class="settings-row">
              <div class="row-description"><strong>请求超时</strong><p>单次 AI 请求的最长等待时间（10–300 秒）。</p></div>
              <n-input-number v-model:value="configForm.timeoutSeconds" :min="10" :max="300" :precision="0" class="timeout-control"><template #suffix>秒</template></n-input-number>
            </div>
          </div>

          <section class="model-section">
            <div class="agent-heading">
              <div><h4>模型列表</h4><p class="section-description">为同一提供商管理可用模型及其 API 能力。</p></div>
              <n-button-group size="small">
                <n-button :loading="discoveringModels" @click="discoverModels"><template #icon><RefreshCw :size="15" /></template>获取模型列表</n-button>
                <n-button title="添加自定义模型" aria-label="添加自定义模型" @click="openNewModel"><template #icon><Plus :size="16" /></template></n-button>
              </n-button-group>
            </div>
            <div v-if="configForm.models.length" class="model-list">
              <article v-for="model in configForm.models" :key="model.id" class="model-card" :class="{ selected: model.id === configForm.activeModelId }">
                <div class="model-card-main"><Sparkles :size="16" /><div><strong>{{ model.name }}</strong><p><code>{{ model.id }}</code> · {{ model.protocol === 'responses' ? 'Responses API' : 'Chat Completions' }}</p></div></div>
                <div class="model-actions"><n-button size="tiny" :disabled="model.id === configForm.activeModelId" @click="selectModel(model.id)">{{ model.id === configForm.activeModelId ? '默认' : '设为默认' }}</n-button><n-button size="tiny" @click="openEditModel(model)">编辑</n-button><n-popconfirm v-if="model.id !== configForm.activeModelId" placement="left" positive-text="删除" negative-text="取消" :style="{ maxWidth: '280px' }" @positive-click="removeModel(model)"><template #trigger><n-button size="tiny" type="error">删除</n-button></template><span class="delete-confirm-text">确定删除模型 “{{ model.name }}”？</span></n-popconfirm></div>
              </article>
            </div>
            <p v-else class="model-empty">尚未添加模型。</p>
          </section>

          <n-modal
            v-model:show="showDiscoveredModels"
            preset="card"
            title="获取模型列表"
            class="discovered-models-dialog"
            :style="discoveredModelsModalStyle"
            :mask-closable="false"
            @update:show="(show) => { if (!show) closeDiscoveredModels() }"
          >
            <n-select v-model:value="pendingDiscoveredModelIds" :options="discoveredModelOptions" multiple filterable placeholder="搜索并选择接口返回的模型" />
            <p class="model-selection-hint">已选择 {{ pendingDiscoveredModelIds.length }} 个模型。</p>
            <div class="provider-actions"><n-button type="primary" :disabled="!pendingDiscoveredModelIds.length" @click="insertDiscoveredModels">插入</n-button><n-button @click="closeDiscoveredModels">取消</n-button></div>
          </n-modal>

          <n-modal
            v-model:show="showModelEditor"
            preset="card"
            :title="editingModel ? '编辑模型' : '添加模型'"
            class="model-editor"
            :style="modelEditorModalStyle"
            :mask-closable="false"
            @update:show="handleModelEditorVisibility"
          >
            <form @submit.prevent="saveModel">
              <n-form label-placement="top" size="small">
                <div class="model-form-grid"><n-form-item label="模型 ID"><n-input v-model:value="modelForm.id" maxlength="160" placeholder="例如 gpt-4.1-mini" required /></n-form-item><n-form-item label="显示名称"><n-input v-model:value="modelForm.name" maxlength="80" placeholder="例如 GPT-4.1 Mini" required /></n-form-item></div>
                <div class="model-form-grid"><n-form-item label="最大上下文 Token"><n-input-number v-model:value="modelForm.maxContextTokens" :min="1" :precision="0" /></n-form-item><n-form-item label="最大输出 Token"><n-input-number v-model:value="modelForm.maxOutputTokens" :min="1" :precision="0" /></n-form-item></div>
                <div class="model-form-grid"><n-form-item label="请求协议"><n-select v-model:value="modelForm.protocol" :options="[{ label: 'Chat Completions', value: 'chat_completions' }, { label: 'Responses API', value: 'responses' }]" /></n-form-item><n-form-item label="推理强度"><n-input v-model:value="modelForm.reasoningEffort" placeholder="例如 medium" :disabled="!modelForm.supportsReasoning" /></n-form-item></div>
                <n-form-item label="提示词缓存 Key"><n-input v-model:value="modelForm.promptCacheKey" placeholder="可选" :disabled="!modelForm.supportsPromptCaching" /></n-form-item>
                <div class="capability-grid"><label><span>工具调用</span><n-switch v-model:value="modelForm.supportsTools" /></label><label><span>图片输入</span><n-switch v-model:value="modelForm.supportsImages" /></label><label><span>并行工具调用</span><n-switch v-model:value="modelForm.supportsParallelToolCalls" :disabled="!modelForm.supportsTools" /></label><label><span>提示词缓存</span><n-switch v-model:value="modelForm.supportsPromptCaching" /></label><label><span>推理</span><n-switch v-model:value="modelForm.supportsReasoning" /></label></div>
              </n-form>
              <div class="provider-actions"><n-button type="primary" attr-type="submit">保存模型</n-button><n-button @click="closeModelEditor">取消</n-button></div>
            </form>
          </n-modal>

          <div class="provider-actions">
            <n-button type="primary" :loading="savingConfig" @click="saveConfig">保存配置</n-button>
            <n-button :disabled="!configured" :loading="testingConnection" @click="testConnection">测试连接</n-button>
          </div>
          <p v-if="connectionTest" class="connection-test-result" :class="connectionTest.status">{{ connectionTest.message }}</p>
          <p class="privacy-notice">你的消息和主动选择的上下文将发送到所配置的第三方 AI 服务。</p>
        </section>
      </n-tab-pane>

      <n-tab-pane name="agent" tab="Agent">
        <section class="tab-section">
          <div class="agent-heading">
            <div><h3>Agent 配置</h3><p class="section-description">Agent 仅影响回答角色与沟通方式，不增加 SSH 执行权限。</p></div>
            <n-button size="small" @click="openNewAgent"><template #icon><Plus :size="15" /></template>新建 Agent</n-button>
          </div>
          <div class="agent-list">
            <article v-for="agent in aiStore.agents" :key="agent.id" class="agent-card" :class="{ selected: agent.id === aiStore.selectedAgentId }">
              <div class="agent-card-main"><Sparkles :size="16" /><div><strong>{{ agent.name }}</strong><p>{{ agent.isDefault ? '默认 Agent' : '自定义 Agent' }}</p></div></div>
              <div class="agent-actions"><n-button size="tiny" @click="aiStore.selectAgent(agent.id)">使用</n-button><n-button size="tiny" @click="openEditAgent(agent)">编辑</n-button><n-popconfirm v-if="!agent.isDefault" positive-text="删除" negative-text="取消" @positive-click="deleteAgent(agent)"><template #trigger><n-button size="tiny" type="error">删除</n-button></template>确定删除 Agent “{{ agent.name }}”？</n-popconfirm></div>
            </article>
          </div>
          <form v-if="showAgentEditor" class="agent-editor" @submit.prevent="saveAgent">
            <h4>{{ editingAgent ? '编辑 Agent' : '新建 Agent' }}</h4>
            <n-form label-placement="top" size="small"><n-form-item label="名称"><n-input v-model:value="agentForm.name" maxlength="80" required /></n-form-item><n-form-item label="提示词"><n-input v-model:value="agentForm.prompt" type="textarea" :autosize="{ minRows: 5, maxRows: 10 }" maxlength="16384" required /></n-form-item></n-form>
            <div class="provider-actions"><n-button type="primary" attr-type="submit" :loading="savingAgent">保存 Agent</n-button><n-button @click="closeAgentEditor">取消</n-button></div>
          </form>
        </section>
      </n-tab-pane>

      <n-tab-pane name="tools" tab="工具接入"><section class="coming-soon"><Sparkles :size="22" /><strong>工具接入即将推出</strong><p>SSH 工具调用将在确认执行审批流程完成后开放。</p></section></n-tab-pane>
      <n-tab-pane name="web-search" tab="网络搜索"><section class="coming-soon"><strong>网络搜索即将推出</strong><p>启用后，AI 可根据你的明确授权检索公开网络信息。</p></section></n-tab-pane>

    </n-tabs>
  </section>
</template>

<style scoped>
.ai-settings { max-width: 760px; }.settings-alert { margin-bottom: 14px; }
.ai-tabs :deep(.n-tabs-nav) { margin-bottom: 24px; }
.ai-tabs :deep(.n-tabs-nav-scroll-content) { gap: 3px; }
.ai-tabs :deep(.n-tabs-tab) { min-width: 104px; min-height: 36px; padding: 0 16px; color: var(--app-muted); font-size: 13px; transition: color .16s ease; }
.ai-tabs :deep(.n-tabs-tab:hover) { color: var(--app-text); }
.ai-tabs :deep(.n-tabs-tab--active) { color: var(--app-text); font-weight: 500; }
.ai-tabs :deep(.n-tabs-tab__label) { display: grid; width: 100%; height: 100%; cursor: pointer; place-items: center; }
.tab-section h3 { margin: 0 0 6px; color: var(--app-text); font-size: 15px; }.section-description, .privacy-notice { margin: 0 0 16px; color: var(--app-muted); font-size: 12px; line-height: 1.6; }.settings-card { border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); overflow: hidden; }.settings-row { display: flex; align-items: center; justify-content: space-between; min-height: 82px; gap: 28px; padding: 14px 16px; border-bottom: 1px solid var(--app-border); }.settings-row:last-child { border-bottom: 0; }.row-description { min-width: 0; }.row-description strong { color: var(--app-text); font-size: 13px; }.row-description p { margin: 5px 0 0; color: var(--app-muted); font-size: 12px; line-height: 1.45; }.row-control { width: min(310px, 48%); flex: 0 0 auto; }.timeout-control { width: 132px; flex: 0 0 auto; }.settings-card :deep(.n-input), .settings-card :deep(.n-input-number), .settings-card :deep(.n-base-selection), .agent-editor :deep(.n-input) { --n-color: var(--app-base) !important; --n-color-focus: var(--app-base) !important; --n-border: 1px solid var(--app-border) !important; --n-border-focus: 1px solid var(--app-accent) !important; --n-text-color: var(--app-text) !important; --n-placeholder-color: var(--app-muted) !important; }.settings-card :deep(.n-input-wrapper), .settings-card :deep(.n-input-number-input), .settings-card :deep(.n-base-selection-label) { background: var(--app-base) !important; }.provider-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-top: 16px; }.configured-status { color: #35b887; font-size: 12px; }.connection-test-result { margin: 10px 0 0; font-size: 12px; }.connection-test-result.success { color: #35b887; }.connection-test-result.authentication_failed, .connection-test-result.model_unavailable, .connection-test-result.rate_limited, .connection-test-result.service_unavailable, .connection-test-result.timeout, .connection-test-result.network_error { color: #d9973f; }.privacy-notice { margin-top: 14px; }.agent-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }.agent-heading .section-description { margin-bottom: 18px; }.agent-list { display: flex; flex-direction: column; gap: 8px; }.agent-card { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); }.agent-card.selected { border-color: var(--app-accent); }.agent-card-main { display: flex; align-items: center; gap: 10px; }.agent-card-main svg { color: var(--app-accent); }.agent-card p { margin: 3px 0 0; color: var(--app-muted); font-size: 11px; }.agent-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }.agent-editor { margin-top: 16px; padding: 18px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); }.agent-editor h4 { margin: 0 0 14px; color: var(--app-text); }.agent-editor :deep(.n-form-item-label) { color: var(--app-muted) !important; }.coming-soon { display: flex; min-height: 180px; flex-direction: column; align-items: center; justify-content: center; padding: 20px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); color: var(--app-text); text-align: center; }.coming-soon svg { margin-bottom: 10px; color: var(--app-accent); }.coming-soon p { margin: 7px 0 0; color: var(--app-muted); font-size: 12px; }
@media (max-width: 620px) { .ai-tabs :deep(.n-tabs-tab) { min-width: auto; padding: 7px 10px; }.settings-row, .model-card { align-items: stretch; flex-direction: column; gap: 12px; }.row-control { width: 100%; }.timeout-control { width: 100%; }.model-actions { justify-content: flex-start; }.model-form-grid, .capability-grid { grid-template-columns: 1fr; } }
.model-section { margin-top: 20px; }.model-section h4 { margin: 0 0 6px; color: var(--app-text); font-size: 14px; }.model-list { display: flex; flex-direction: column; gap: 8px; }.model-card { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); }.model-card.selected { border-color: var(--app-accent); }.model-card-main { display: flex; min-width: 0; align-items: center; gap: 10px; }.model-card-main svg { flex: 0 0 auto; color: var(--app-accent); }.model-card-main strong { color: var(--app-text); font-size: 13px; }.model-card-main p, .model-empty { margin: 3px 0 0; color: var(--app-muted); font-size: 11px; }.model-card-main code { color: inherit; }.model-actions { display: flex; flex: 0 0 auto; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }.discovered-models-dialog, .model-editor { width: min(680px, calc(100vw - 32px)); max-height: calc(100vh - 32px); overflow: auto; padding: 20px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); box-shadow: 0 12px 30px var(--app-shadow); }.discovered-models-dialog { width: min(520px, calc(100vw - 32px)); }.discovered-models-dialog h4, .model-editor h4 { margin: 0 0 16px; color: var(--app-text); font-size: 16px; }.discovered-models-dialog :deep(.n-base-selection), .model-editor :deep(.n-input), .model-editor :deep(.n-input-number), .model-editor :deep(.n-base-selection) { --n-color: var(--app-base) !important; --n-color-focus: var(--app-base) !important; --n-border: 1px solid var(--app-border) !important; --n-border-focus: 1px solid var(--app-accent) !important; --n-text-color: var(--app-text) !important; --n-placeholder-color: var(--app-muted) !important; }.discovered-models-dialog :deep(.n-base-selection-label), .model-editor :deep(.n-input-wrapper), .model-editor :deep(.n-input-number-input), .model-editor :deep(.n-base-selection-label) { background: var(--app-base) !important; }.model-editor :deep(.n-form-item-label) { color: var(--app-muted) !important; }.add-discovered-model { margin-top: 12px; }.pending-model-list { display: flex; flex-direction: column; gap: 6px; margin-top: 16px; }.pending-model-item { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 36px; padding: 0 8px 0 12px; border: 1px solid var(--app-border); border-radius: 6px; background: var(--app-base); color: var(--app-text); }.pending-model-item code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.model-form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }.capability-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }.capability-grid label { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-width: 0; color: var(--app-text); font-size: 12px; }
.discovered-models-dialog :deep(.n-base-selection) { --n-color: var(--app-base) !important; --n-color-focus: var(--app-base) !important; --n-border: 1px solid var(--app-border) !important; --n-border-focus: 1px solid var(--app-accent) !important; --n-text-color: var(--app-text) !important; --n-placeholder-color: var(--app-muted) !important; }.discovered-models-dialog :deep(.n-base-selection-label) { background: var(--app-base) !important; }.model-selection-hint { margin: 10px 0 0; color: var(--app-muted); font-size: 12px; }.delete-confirm-text { overflow-wrap: anywhere; }
</style>
