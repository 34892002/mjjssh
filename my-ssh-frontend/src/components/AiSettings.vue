<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  NButton,
  NButtonGroup,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  useMessage,
  useThemeVars,
  NPopconfirm,
  NSelect,
  NSwitch,
  NTabPane,
  NTabs,
  NCheckbox,
  NCheckboxGroup,
} from 'naive-ui'
import { Plus, RefreshCw, Sparkles } from '@lucide/vue'
import FloatingPanel from './FloatingPanel.vue'
import { useAiStore } from '../stores/ai'
import type { AiAgentConfig, AiConnectionTestResult, AiModelConfig } from '../types/ai'
import { useLocale } from '../composables/useLocale'

const aiStore = useAiStore()
const { t } = useLocale()
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

const activeTab = ref('provider')
const configForm = ref({ baseUrl: '', apiKey: '', model: '', models: [] as AiModelConfig[], activeModelId: null as string | null, timeoutSeconds: 60 })
const editingModel = ref<AiModelConfig | null>(null)
const showModelEditor = ref(false)
const modelForm = ref<AiModelConfig>(createModel())
const savingConfig = ref(false)
const testingConnection = ref(false)
const discoveringModels = ref(false)
const discoveredModelIds = ref<string[]>([])
const selectedDiscoveredModelIds = ref<string[]>([])
const modelSearch = ref('')
const showModelPicker = ref(false)

const connectionTest = ref<AiConnectionTestResult | null>(null)
const editingAgent = ref<AiAgentConfig | null>(null)
const showAgentEditor = ref(false)
const agentForm = ref({ name: '', prompt: '' })
const savingAgent = ref(false)
const configured = computed(() => aiStore.config.configured)
const providerOptions = [{ label: 'OpenAI-compatible', value: 'openai_compatible' }]
const activeModel = computed(() => configForm.value.models.find((model) => model.id === configForm.value.activeModelId) ?? null)
const availableDiscoveredModels = computed(() => {
  const enabledIds = new Set(configForm.value.models.map((model) => model.id))
  const search = modelSearch.value.trim().toLowerCase()
  return discoveredModelIds.value.filter((id) => !enabledIds.has(id) && (!search || id.toLowerCase().includes(search)))
})

watch(() => aiStore.error, (error) => {
  if (error) message.error(error)
})

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
  discoveredModelIds.value = []
  selectedDiscoveredModelIds.value = []
  showModelPicker.value = false
}

function selectModel(modelId: string | null) {
  const model = configForm.value.models.find((item) => item.id === modelId)
  configForm.value.activeModelId = model?.id ?? null
  configForm.value.model = model?.name ?? ''
}

function closeModelPicker() {
  showModelPicker.value = false
  modelSearch.value = ''
  selectedDiscoveredModelIds.value = []
}

function insertDiscoveredModels() {
  const existingIds = new Set(configForm.value.models.map((model) => model.id))
  const addedModels = selectedDiscoveredModelIds.value
    .filter((id) => !existingIds.has(id))
    .map((id) => ({ ...createModel(), id, name: id }))
  if (!addedModels.length) {
    message.warning(t('ai.selectAtLeastOneModel'))
    return
  }
  configForm.value.models.push(...addedModels)
  if (!configForm.value.activeModelId) selectModel(addedModels[0].id)
  message.success(t('ai.modelsInserted', { count: addedModels.length }))
  closeModelPicker()
}

async function discoverModels() {
  if (discoveringModels.value) return
  if (!configForm.value.baseUrl.trim()) {
    message.warning(t('ai.apiUrlRequired'))
    return
  }
  if (!configForm.value.apiKey.trim() && !configured.value) {
    message.warning(t('ai.apiKeyRequired'))
    return
  }

  discoveringModels.value = true
  try {
    const models = await aiStore.discoverModels({
      baseUrl: configForm.value.baseUrl,
      apiKey: configForm.value.apiKey,
      timeoutSeconds: configForm.value.timeoutSeconds,
    })
    const uniqueModels = [...new Set(models)]
    if (!uniqueModels.length) {
      message.warning(t('ai.noModelsReturned'))
      return
    }
    discoveredModelIds.value = uniqueModels.sort()
    selectedDiscoveredModelIds.value = []
    modelSearch.value = ''
    showModelPicker.value = true
  } catch {
    // The store error watcher displays the request failure.
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



function saveModel() {
  const model = { ...modelForm.value, id: modelForm.value.id.trim(), name: modelForm.value.name.trim() }
  if (!model.id || !model.name) return
  const duplicate = configForm.value.models.some((item) =>
    item.id === model.id && item.id !== editingModel.value?.id ||
    item.name === model.name && item.id !== editingModel.value?.id,
  )
  if (duplicate) {
    message.error(t('ai.duplicateModel'))
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
    message.warning(t('ai.selectAnotherDefaultModel'))
    return
  }
  const index = configForm.value.models.findIndex((item) => item.id === model.id)
  if (index >= 0) configForm.value.models.splice(index, 1)
}

onMounted(async () => {
  await Promise.all([aiStore.loadConfigStatus(), aiStore.loadAgents()])
  syncConfigForm()
})

async function saveConfig() {
  if (!configured.value && !configForm.value.apiKey.trim()) {
    message.warning(t('ai.apiKeyRequiredForInitialSave'))
    return
  }

  if (!activeModel.value) {
    message.warning(t('ai.addAndSelectModel'))
    return
  }

  configForm.value.model = activeModel.value.name
  savingConfig.value = true
  connectionTest.value = null
  try {
    await aiStore.saveConfig(configForm.value)
    syncConfigForm()
    message.success(t('ai.configSaved'))
  } catch {
    // The store error watcher displays the request failure.
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
  } catch {
    // The store error watcher displays the request failure.
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
    message.success(t('ai.agentSaved'))
  } catch {
    // The store error watcher displays the request failure.
  } finally {
    savingAgent.value = false
  }
}

async function deleteAgent(agent: AiAgentConfig) {
  try {
    await aiStore.deleteAgent(agent.id)
    if (editingAgent.value?.id === agent.id) closeAgentEditor()
    message.success(t('ai.agentDeleted'))
  } catch {
    // The store error watcher displays the request failure.
  }
}
</script>

<template>
  <section class="ai-settings">
    <n-tabs v-model:value="activeTab" type="line" class="ai-tabs" animated>
      <n-tab-pane name="provider" :tab="t('ai.providerTab')">
        <section class="tab-section">
          <h3>{{ t('ai.providerTitle') }}</h3>
          <p class="section-description">{{ t('ai.providerDescription') }}</p>

          <div class="settings-card">
            <div class="settings-row">
              <div class="row-description"><strong>{{ t('ai.provider') }}</strong><p>{{ t('ai.providerHelp') }}</p></div>
              <n-select :value="'openai_compatible'" :options="providerOptions" disabled class="row-control" />
            </div>
            <div class="settings-row">
              <div class="row-description"><strong>{{ t('ai.apiKey') }}</strong><p>{{ t('ai.apiKeyHelp') }}</p></div>
              <n-input v-model:value="configForm.apiKey" type="password" show-password-on="click" autocomplete="off" :placeholder="configured ? t('ai.apiKeyUpdatePlaceholder') : t('ai.apiKeyPlaceholder')" class="row-control" />
            </div>
            <div class="settings-row">
              <div class="row-description"><strong>{{ t('ai.apiUrl') }}</strong><p>{{ t('ai.apiUrlHelp') }}</p></div>
              <n-input v-model:value="configForm.baseUrl" placeholder="https://api.openai.com/v1" class="row-control" />
            </div>

            <div class="settings-row">
              <div class="row-description"><strong>{{ t('ai.requestTimeout') }}</strong><p>{{ t('ai.requestTimeoutHelp') }}</p></div>
              <n-input-number v-model:value="configForm.timeoutSeconds" :min="10" :max="300" :precision="0" class="timeout-control"><template #suffix>{{ t('ai.seconds') }}</template></n-input-number>
            </div>
          </div>

          <section class="model-section">
            <div class="agent-heading">
              <div><h4>{{ t('ai.modelList') }}</h4><p class="section-description">{{ t('ai.modelListDescription') }}</p></div>
              <n-button-group size="small">
                <n-button :loading="discoveringModels" @click="discoverModels"><template #icon><RefreshCw :size="15" /></template>{{ t('ai.discoverModels') }}</n-button>
                <n-button :title="t('ai.addCustomModel')" :aria-label="t('ai.addCustomModel')" @click="openNewModel"><template #icon><Plus :size="16" /></template></n-button>
              </n-button-group>
            </div>
            <div v-if="configForm.models.length" class="model-list">
              <article v-for="model in configForm.models" :key="model.id" class="model-card" :class="{ selected: model.id === configForm.activeModelId }">
                <div class="model-card-main"><Sparkles :size="16" /><div><strong>{{ model.name }}</strong><p><code>{{ model.id }}</code> · {{ model.protocol === 'responses' ? 'Responses API' : 'Chat Completions' }}</p></div></div>
                <div class="model-actions"><n-button size="tiny" :disabled="model.id === configForm.activeModelId" @click="selectModel(model.id)">{{ model.id === configForm.activeModelId ? t('ai.default') : t('ai.setDefault') }}</n-button><n-button size="tiny" @click="openEditModel(model)">{{ t('ai.edit') }}</n-button><n-popconfirm v-if="model.id !== configForm.activeModelId" placement="left" :positive-text="t('ai.delete')" :negative-text="t('ai.cancel')" :style="{ maxWidth: '280px' }" @positive-click="removeModel(model)"><template #trigger><n-button size="tiny" type="error">{{ t('ai.delete') }}</n-button></template><span class="delete-confirm-text">{{ t('ai.deleteModelConfirm', { name: model.name }) }}</span></n-popconfirm></div>
              </article>
            </div>
            <p v-else class="model-empty">{{ t('ai.noModels') }}</p>
          </section>

          <FloatingPanel :show="showModelPicker" :title="t('ai.selectModelsToInsert')" @close="closeModelPicker">
            <div class="model-picker-summary">{{ t('ai.availableModels', { count: availableDiscoveredModels.length }) }}</div>
            <n-input v-model:value="modelSearch" clearable :placeholder="t('ai.searchModelId')" />
            <n-checkbox-group v-model:value="selectedDiscoveredModelIds" class="model-picker-list">
              <n-checkbox v-for="modelId in availableDiscoveredModels" :key="modelId" :value="modelId">{{ modelId }}</n-checkbox>
              <p v-if="!availableDiscoveredModels.length" class="model-empty">{{ t('ai.noMatchingModels') }}</p>
            </n-checkbox-group>
            <div class="model-picker-actions"><span>{{ t('ai.selectedModels', { count: selectedDiscoveredModelIds.length }) }}</span><n-button @click="closeModelPicker">{{ t('ai.cancel') }}</n-button><n-button type="primary" @click="insertDiscoveredModels">{{ t('ai.insertModels') }}</n-button></div>
          </FloatingPanel>

          <FloatingPanel :show="showModelEditor" :title="editingModel ? t('ai.editModel') : t('ai.addModel')" width="720px" @close="closeModelEditor">
            <form class="model-editor" @submit.prevent="saveModel">
              <n-form label-placement="top" size="small">
                <div class="model-form-grid"><n-form-item :label="t('ai.modelId')"><n-input v-model:value="modelForm.id" maxlength="160" :placeholder="t('ai.modelIdPlaceholder')" required /></n-form-item><n-form-item :label="t('ai.displayName')"><n-input v-model:value="modelForm.name" maxlength="80" :placeholder="t('ai.displayNamePlaceholder')" required /></n-form-item></div>
                <div class="model-form-grid"><n-form-item :label="t('ai.maxContextTokens')"><n-input-number v-model:value="modelForm.maxContextTokens" :min="1" :precision="0" /></n-form-item><n-form-item :label="t('ai.maxOutputTokens')"><n-input-number v-model:value="modelForm.maxOutputTokens" :min="1" :precision="0" /></n-form-item></div>
                <div class="model-form-grid"><n-form-item :label="t('ai.requestProtocol')"><n-select v-model:value="modelForm.protocol" :options="[{ label: 'Chat Completions', value: 'chat_completions' }, { label: 'Responses API', value: 'responses' }]" /></n-form-item><n-form-item :label="t('ai.reasoningEffort')"><n-input v-model:value="modelForm.reasoningEffort" :placeholder="t('ai.reasoningEffortPlaceholder')" :disabled="!modelForm.supportsReasoning" /></n-form-item></div>
                <n-form-item :label="t('ai.promptCacheKey')"><n-input v-model:value="modelForm.promptCacheKey" :placeholder="t('ai.optional')" :disabled="!modelForm.supportsPromptCaching" /></n-form-item>
                <div class="capability-grid"><label><span>{{ t('ai.toolCalling') }}</span><n-switch v-model:value="modelForm.supportsTools" /></label><label><span>{{ t('ai.imageInput') }}</span><n-switch v-model:value="modelForm.supportsImages" /></label><label><span>{{ t('ai.parallelToolCalls') }}</span><n-switch v-model:value="modelForm.supportsParallelToolCalls" :disabled="!modelForm.supportsTools" /></label><label><span>{{ t('ai.promptCaching') }}</span><n-switch v-model:value="modelForm.supportsPromptCaching" /></label><label><span>{{ t('ai.reasoning') }}</span><n-switch v-model:value="modelForm.supportsReasoning" /></label></div>
              </n-form>
              <div class="provider-actions"><n-button type="primary" attr-type="submit">{{ t('ai.saveModel') }}</n-button><n-button @click="closeModelEditor">{{ t('ai.cancel') }}</n-button></div>
            </form>
          </FloatingPanel>

          <div class="provider-actions">
            <n-button type="primary" :loading="savingConfig" @click="saveConfig">{{ t('ai.saveConfig') }}</n-button>
            <n-button :disabled="!configured" :loading="testingConnection" @click="testConnection">{{ t('ai.testConnection') }}</n-button>
          </div>
          <p v-if="connectionTest" class="connection-test-result" :class="connectionTest.status">{{ connectionTest.message }}</p>
          <p class="privacy-notice">{{ t('ai.privacyNotice') }}</p>
        </section>
      </n-tab-pane>

      <n-tab-pane name="agent" tab="Agent">
        <section class="tab-section">
          <div class="agent-heading">
            <div><h3>{{ t('ai.agentConfiguration') }}</h3><p class="section-description">{{ t('ai.agentDescription') }}</p></div>
            <n-button size="small" @click="openNewAgent"><template #icon><Plus :size="15" /></template>{{ t('ai.newAgent') }}</n-button>
          </div>
          <div class="agent-list">
            <article v-for="agent in aiStore.agents" :key="agent.id" class="agent-card" :class="{ selected: agent.id === aiStore.selectedAgentId }">
              <div class="agent-card-main"><Sparkles :size="16" /><div><strong>{{ agent.name }}</strong><p>{{ agent.isDefault ? t('ai.defaultAgent') : t('ai.customAgent') }}</p></div></div>
              <div class="agent-actions"><n-button size="tiny" @click="aiStore.selectAgent(agent.id)">{{ t('ai.use') }}</n-button><n-button size="tiny" @click="openEditAgent(agent)">{{ t('ai.edit') }}</n-button><n-popconfirm v-if="!agent.isDefault" :positive-text="t('ai.delete')" :negative-text="t('ai.cancel')" @positive-click="deleteAgent(agent)"><template #trigger><n-button size="tiny" type="error">{{ t('ai.delete') }}</n-button></template>{{ t('ai.deleteAgentConfirm', { name: agent.name }) }}</n-popconfirm></div>
            </article>
          </div>
          <form v-if="showAgentEditor" class="agent-editor" @submit.prevent="saveAgent">
            <h4>{{ editingAgent ? t('ai.editAgent') : t('ai.newAgent') }}</h4>
            <n-form label-placement="top" size="small"><n-form-item :label="t('ai.name')"><n-input v-model:value="agentForm.name" maxlength="80" required /></n-form-item><n-form-item :label="t('ai.prompt')"><n-input v-model:value="agentForm.prompt" type="textarea" :autosize="{ minRows: 5, maxRows: 10 }" maxlength="16384" required /></n-form-item></n-form>
            <div class="provider-actions"><n-button type="primary" attr-type="submit" :loading="savingAgent">{{ t('ai.saveAgent') }}</n-button><n-button @click="closeAgentEditor">{{ t('ai.cancel') }}</n-button></div>
          </form>
        </section>
      </n-tab-pane>

      <n-tab-pane name="tools" :tab="t('ai.toolsTab')"><section class="coming-soon"><Sparkles :size="22" /><strong>{{ t('ai.toolsComingSoon') }}</strong><p>{{ t('ai.toolsComingSoonDescription') }}</p></section></n-tab-pane>
      <n-tab-pane name="web-search" :tab="t('ai.webSearchTab')"><section class="coming-soon"><strong>{{ t('ai.webSearchComingSoon') }}</strong><p>{{ t('ai.webSearchComingSoonDescription') }}</p></section></n-tab-pane>

    </n-tabs>
  </section>
</template>

<style scoped>
.ai-settings { max-width: 760px; }
.ai-tabs :deep(.n-tabs-nav) { margin-bottom: 24px; }
.ai-tabs :deep(.n-tabs-nav-scroll-content) { gap: 3px; }
.ai-tabs :deep(.n-tabs-tab) { min-width: 104px; min-height: 36px; padding: 0 16px; color: var(--app-muted); font-size: 13px; transition: color .16s ease; }
.ai-tabs :deep(.n-tabs-tab:hover) { color: var(--app-text); }
.ai-tabs :deep(.n-tabs-tab--active) { color: var(--app-text); font-weight: 500; }
.ai-tabs :deep(.n-tabs-tab__label) { display: grid; width: 100%; height: 100%; cursor: pointer; place-items: center; }
.tab-section h3 { margin: 0 0 6px; color: var(--app-text); font-size: 15px; }.section-description, .privacy-notice { margin: 0 0 16px; color: var(--app-muted); font-size: 12px; line-height: 1.6; }.settings-card { border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); overflow: hidden; }.settings-row { display: flex; align-items: center; justify-content: space-between; min-height: 82px; gap: 28px; padding: 14px 16px; border-bottom: 1px solid var(--app-border); }.settings-row:last-child { border-bottom: 0; }.row-description { min-width: 0; }.row-description strong { color: var(--app-text); font-size: 13px; }.row-description p { margin: 5px 0 0; color: var(--app-muted); font-size: 12px; line-height: 1.45; }.row-control { width: min(310px, 48%); flex: 0 0 auto; }.timeout-control { width: 132px; flex: 0 0 auto; }.settings-card :deep(.n-input), .settings-card :deep(.n-input-number), .settings-card :deep(.n-base-selection), .agent-editor :deep(.n-input) { --n-color: var(--app-base) !important; --n-color-focus: var(--app-base) !important; --n-border: 1px solid var(--app-border) !important; --n-border-focus: 1px solid var(--app-accent) !important; --n-text-color: var(--app-text) !important; --n-placeholder-color: var(--app-muted) !important; }.settings-card :deep(.n-input-wrapper), .settings-card :deep(.n-input-number-input), .settings-card :deep(.n-base-selection-label) { background: var(--app-base) !important; }.provider-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-top: 16px; }.configured-status { color: #35b887; font-size: 12px; }.connection-test-result { margin: 10px 0 0; font-size: 12px; }.connection-test-result.success { color: #35b887; }.connection-test-result.authentication_failed, .connection-test-result.model_unavailable, .connection-test-result.rate_limited, .connection-test-result.service_unavailable, .connection-test-result.timeout, .connection-test-result.network_error { color: #d9973f; }.privacy-notice { margin-top: 14px; }.agent-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }.agent-heading .section-description { margin-bottom: 18px; }.agent-list { display: flex; flex-direction: column; gap: 8px; }.agent-card { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); }.agent-card.selected { border-color: var(--app-accent); }.agent-card-main { display: flex; align-items: center; gap: 10px; }.agent-card-main svg { color: var(--app-accent); }.agent-card p { margin: 3px 0 0; color: var(--app-muted); font-size: 11px; }.agent-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }.agent-editor { margin-top: 16px; padding: 18px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); }.agent-editor h4 { margin: 0 0 14px; color: var(--app-text); }.agent-editor :deep(.n-form-item-label) { color: var(--app-muted) !important; }.coming-soon { display: flex; min-height: 180px; flex-direction: column; align-items: center; justify-content: center; padding: 20px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); color: var(--app-text); text-align: center; }.coming-soon svg { margin-bottom: 10px; color: var(--app-accent); }.coming-soon p { margin: 7px 0 0; color: var(--app-muted); font-size: 12px; }
@media (max-width: 620px) { .ai-tabs :deep(.n-tabs-tab) { min-width: auto; padding: 7px 10px; }.settings-row, .model-card { align-items: stretch; flex-direction: column; gap: 12px; }.row-control { width: 100%; }.timeout-control { width: 100%; }.model-actions { justify-content: flex-start; }.model-form-grid, .capability-grid { grid-template-columns: 1fr; } }
.model-section { margin-top: 20px; }.model-section h4 { margin: 0 0 6px; color: var(--app-text); font-size: 14px; }.model-list { display: flex; flex-direction: column; gap: 8px; }.model-card { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); }.model-card.selected { border-color: var(--app-accent); }.model-card-main { display: flex; min-width: 0; align-items: center; gap: 10px; }.model-card-main svg { flex: 0 0 auto; color: var(--app-accent); }.model-card-main strong { color: var(--app-text); font-size: 13px; }.model-card-main p, .model-empty { margin: 3px 0 0; color: var(--app-muted); font-size: 11px; }.model-card-main code { color: inherit; }.model-actions { display: flex; flex: 0 0 auto; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }.model-picker-summary { margin-bottom: 12px; color: var(--app-muted); font-size: 12px; }.model-picker-list { display: grid; min-height: 0; max-height: 340px; gap: 9px; overflow-y: auto; padding: 4px 2px; }.model-picker-list :deep(.n-checkbox) { min-height: 28px; }.model-picker-actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }.model-picker-actions span { margin-right: auto; color: var(--app-muted); font-size: 12px; }.model-editor { margin-top: 16px; padding: 18px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-surface); }.model-editor h4 { margin: 0 0 16px; color: var(--app-text); font-size: 16px; }.model-editor :deep(.n-input), .model-editor :deep(.n-input-number), .model-editor :deep(.n-base-selection) { --n-color: var(--app-base) !important; --n-color-focus: var(--app-base) !important; --n-border: 1px solid var(--app-border) !important; --n-border-focus: 1px solid var(--app-accent) !important; --n-text-color: var(--app-text) !important; --n-placeholder-color: var(--app-muted) !important; }.model-editor :deep(.n-input-wrapper), .model-editor :deep(.n-input-number-input), .model-editor :deep(.n-base-selection-label) { background: var(--app-base) !important; }.model-editor :deep(.n-form-item-label) { color: var(--app-muted) !important; }.model-form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }.capability-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }.capability-grid label { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-width: 0; color: var(--app-text); font-size: 12px; }.delete-confirm-text { overflow-wrap: anywhere; }
</style>
