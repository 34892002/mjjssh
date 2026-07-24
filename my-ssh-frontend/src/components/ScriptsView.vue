<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Copy, Edit3, Plus, RefreshCw, Search, Trash2 } from '@lucide/vue'
import {
  NAlert,
  NButton,
  NEmpty,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NPopconfirm,
  NSelect,
  NSpace,
  NSwitch,
  NTabPane,
  NTabs,
  NTag,
} from 'naive-ui'
import { useLocale } from '../composables/useLocale'
import { useScriptStore } from '../stores/scripts'

import type { CreateScriptRequest, ScriptRiskLevel, ScriptView, SubscriptionScriptView } from '../types'

const scriptStore = useScriptStore()
const { t } = useLocale()

const activeTab = ref<'all' | 'local' | 'subscriptions'>('all')
const allSearch = ref('')
const allSubscriptionScripts = ref<SubscriptionScriptView[]>([])
const selectedSubscriptionId = ref<string | null>(null)
const showScriptForm = ref(false)
const showSubscriptionForm = ref(false)
const editingScript = ref<ScriptView | null>(null)
const formError = ref('')
const scriptForm = ref<CreateScriptRequest>(newScriptForm())
const subscriptionForm = ref({ name: '', url: '' })

function newScriptForm(): CreateScriptRequest {
  return { name: '', description: '', tags: [], command: '', riskLevel: 'medium' }
}

const riskOptions = computed(() => [
  { label: t('scripts.riskLow'), value: 'low' },
  { label: t('scripts.riskMedium'), value: 'medium' },
  { label: t('scripts.riskHigh'), value: 'high' },
])



const selectedSubscription = computed(() =>
  scriptStore.subscriptions.find((item) => item.id === selectedSubscriptionId.value) ?? null,
)

function matchesSearch(script: { name: string; description: string | null; tags: string[]; source?: string }, search: string) {
  const query = search.trim().toLocaleLowerCase()
  if (!query) return true
  return [script.name, script.description ?? '', script.source ?? '', ...script.tags]
    .some((value) => value.toLocaleLowerCase().includes(query))
}

const filteredSubscriptionScripts = computed(() => scriptStore.subscriptionScripts)
const subscriptionNames = computed(() => new Map(scriptStore.subscriptions.map((subscription) => [subscription.id, subscription.name])))
const allScripts = computed(() => [
  ...scriptStore.scripts.map((script) => ({
    ...script,
    source: t('scripts.myScripts'),
    subscription: false as const,
    subscriptionId: null,
    version: null,
  })),
  ...allSubscriptionScripts.value.map((script) => ({
    ...script,
    source: subscriptionNames.value.get(script.subscriptionId) ?? t('scripts.panelSubscriptionScripts'),
    subscription: true as const,
  })),
])
const filteredAllScripts = computed(() => allScripts.value.filter((script) => matchesSearch(script, allSearch.value)))

async function loadAllSubscriptionScripts() {
  const cachedResults = await Promise.all(scriptStore.subscriptions.map((subscription) =>
    invoke<SubscriptionScriptView[]>('list_cached_subscription_scripts', { id: subscription.id }).catch(() => []),
  ))
  allSubscriptionScripts.value = cachedResults.flat()
}

onMounted(async () => {
  await Promise.all([scriptStore.loadScripts(), scriptStore.loadSubscriptions()])
  await loadAllSubscriptionScripts()
})

watch(selectedSubscriptionId, (id) => {
  if (id) void scriptStore.loadSubscriptionScripts(id)
  else scriptStore.subscriptionScripts = []
})

function openCreateScript() {
  editingScript.value = null
  scriptForm.value = newScriptForm()
  formError.value = ''
  showScriptForm.value = true
}

function openEditScript(script: ScriptView) {
  editingScript.value = script
  scriptForm.value = {
    name: script.name,
    description: script.description,
    tags: [...script.tags],
    command: script.command,
    riskLevel: script.riskLevel,
  }
  formError.value = ''
  showScriptForm.value = true
}

function normalizedScriptForm(): CreateScriptRequest | null {
  const name = scriptForm.value.name.trim()
  const description = scriptForm.value.description?.trim() || null
  const tags = (scriptForm.value.tags ?? []).map((tag) => tag.trim()).filter(Boolean)
  const command = scriptForm.value.command
  if (!name || name.length > 80) {
    formError.value = t('scripts.nameInvalid')
    return null
  }
  if (description && description.length > 500) {
    formError.value = t('scripts.descriptionTooLong')
    return null
  }
  if (tags.length > 10 || tags.some((tag) => tag.length > 32)) {
    formError.value = t('scripts.tagsInvalid')
    return null
  }
  if (!command || new TextEncoder().encode(command).byteLength > 32 * 1024) {
    formError.value = t('scripts.commandInvalid')
    return null
  }
  return { name, description, tags, command, riskLevel: scriptForm.value.riskLevel }
}

async function saveScript() {
  formError.value = ''
  const request = normalizedScriptForm()
  if (!request) return
  const result = editingScript.value
    ? await scriptStore.updateScript(editingScript.value.id, request)
    : await scriptStore.createScript(request)
  if (result) showScriptForm.value = false
  else formError.value = scriptStore.error || t('scripts.saveFailed')
}

async function copyScript(script: Pick<ScriptView, 'name' | 'description' | 'tags' | 'command' | 'riskLevel'>) {
  const result = await scriptStore.createScript({
    name: `${script.name} copy`,
    description: script.description,
    tags: [...script.tags],
    command: script.command,
    riskLevel: script.riskLevel,
  })
  if (result) activeTab.value = 'local'
}



async function saveSubscription() {
  const name = subscriptionForm.value.name.trim()
  const url = subscriptionForm.value.url.trim()
  if (!name || !url) {
    formError.value = t('scripts.subscriptionRequired')
    return
  }
  if (!url.startsWith('https://')) {
    formError.value = t('scripts.subscriptionHttpsOnly')
    return
  }
  formError.value = ''
  const subscription = await scriptStore.createSubscription({ name, url })
  if (subscription) {
    selectedSubscriptionId.value = subscription.id
    showSubscriptionForm.value = false
  } else {
    formError.value = scriptStore.error || t('scripts.subscriptionSaveFailed')
  }
}

async function toggleSubscription(subscriptionId: string, enabled: boolean) {
  await scriptStore.updateSubscription(subscriptionId, { enabled })
}

async function refreshSubscription(subscriptionId: string) {
  if (await scriptStore.refreshSubscription(subscriptionId)) await loadAllSubscriptionScripts()
}

async function deleteSubscription(subscriptionId: string) {
  if (await scriptStore.deleteSubscription(subscriptionId)) {
    if (selectedSubscriptionId.value === subscriptionId) selectedSubscriptionId.value = null
    await loadAllSubscriptionScripts()
  }
}

function tagType(level: ScriptRiskLevel) {
  return level === 'high' ? 'error' : level === 'medium' ? 'warning' : 'success'
}
</script>

<template>
  <div class="scripts-view">
    <div class="scripts-header">
      <div>
        <h2>{{ t('scripts.title') }}</h2>
        <p>{{ t('scripts.description') }}</p>
      </div>
      <n-button v-if="activeTab === 'local'" type="primary" @click="openCreateScript">
        <template #icon><Plus :size="16" /></template>
        {{ t('scripts.new') }}
      </n-button>
      <n-button v-else-if="activeTab === 'subscriptions'" type="primary" @click="subscriptionForm = { name: '', url: '' }; formError = ''; showSubscriptionForm = true">
        <template #icon><Plus :size="16" /></template>
        {{ t('scripts.addSubscription') }}
      </n-button>
    </div>

    <n-alert v-if="scriptStore.error" type="error" closable style="margin-bottom: 16px" @close="scriptStore.error = null">
      {{ scriptStore.error }}
    </n-alert>

    <n-tabs v-model:value="activeTab" type="line" animated>
      <n-tab-pane name="all" :tab="t('scripts.all')">
        <div class="script-toolbar">
          <n-input v-model:value="allSearch" clearable :placeholder="t('scripts.search')">
            <template #prefix><Search :size="16" /></template>
          </n-input>
        </div>
        <n-empty v-if="!scriptStore.loading && filteredAllScripts.length === 0" :description="t('scripts.noSearchResults')" />
        <div v-else class="script-list">
          <article v-for="script in filteredAllScripts" :key="script.subscription ? `${script.subscriptionId}:${script.id}` : script.id" class="script-card">
            <div class="script-card-heading">
              <div>
                <h3>{{ script.name }}</h3>
                <p v-if="script.description">{{ script.description }}</p>
              </div>
              <n-tag size="small" :type="tagType(script.riskLevel)">{{ script.riskLevel }}</n-tag>
            </div>
            <div class="tag-row">
              <n-tag size="small" :bordered="false">{{ script.source }}</n-tag>
              <n-tag v-if="script.subscription" size="small" :bordered="false">{{ script.version }}</n-tag>
              <n-tag v-for="tag in script.tags" :key="tag" size="small" :bordered="false">{{ tag }}</n-tag>
            </div>
            <div class="script-actions">
              <n-button v-if="script.subscription" size="small" secondary @click="copyScript(script)"><template #icon><Copy :size="14" /></template>{{ t('scripts.copyToMine') }}</n-button>
              <n-button v-else size="small" secondary @click="openEditScript(script)"><template #icon><Edit3 :size="14" /></template>{{ t('scripts.edit') }}</n-button>
            </div>
          </article>
        </div>
      </n-tab-pane>

      <n-tab-pane name="local" :tab="t('scripts.myScripts')">
        <n-empty v-if="!scriptStore.loading && scriptStore.scripts.length === 0" :description="t('scripts.noLocalScripts')">
          <template #extra><n-button type="primary" @click="openCreateScript">{{ t('scripts.create') }}</n-button></template>
        </n-empty>
        <div v-else class="script-list">
          <article v-for="script in scriptStore.scripts" :key="script.id" class="script-card">
            <div class="script-card-heading">
              <div>
                <h3>{{ script.name }}</h3>
                <p v-if="script.description">{{ script.description }}</p>
              </div>
              <n-tag size="small" :type="tagType(script.riskLevel)">{{ script.riskLevel }}</n-tag>
            </div>
            <div v-if="script.tags.length" class="tag-row"><n-tag v-for="tag in script.tags" :key="tag" size="small" :bordered="false">{{ tag }}</n-tag></div>
            <div class="script-actions">
              <n-button size="small" secondary @click="openEditScript(script)" :title="t('scripts.edit')" :aria-label="t('scripts.edit')"><template #icon><Edit3 :size="14" /></template></n-button>
              <n-button size="small" secondary @click="copyScript(script)" :title="t('scripts.copy')" :aria-label="t('scripts.copy')"><template #icon><Copy :size="14" /></template></n-button>
              <n-popconfirm @positive-click="scriptStore.deleteScript(script.id)">
                <template #trigger><n-button size="small" secondary type="error" :title="t('scripts.delete')" :aria-label="t('scripts.delete')"><template #icon><Trash2 :size="14" /></template></n-button></template>
                {{ t('scripts.deleteConfirm', { name: script.name }) }}
              </n-popconfirm>
            </div>
          </article>
        </div>
      </n-tab-pane>

      <n-tab-pane name="subscriptions" :tab="t('scripts.subscriptions')">
        <div class="subscriptions-layout">
          <aside class="subscription-list">
            <n-empty v-if="!scriptStore.loading && scriptStore.subscriptions.length === 0" :description="t('scripts.noSubscriptions')" size="small" />
            <button v-for="subscription in scriptStore.subscriptions" :key="subscription.id" class="subscription-item" :class="{ selected: selectedSubscriptionId === subscription.id }" @click="selectedSubscriptionId = subscription.id">
              <strong>{{ subscription.name }}</strong>
              {{ subscription.enabled ? t('scripts.enabled') : t('scripts.disabled') }}
            </button>
          </aside>
          <section class="subscription-content">
            <template v-if="selectedSubscription">
              <div class="subscription-detail">
                <div><h3>{{ selectedSubscription.name }}</h3><p>{{ selectedSubscription.url }}</p></div>
                <n-space>
                  <n-switch :value="selectedSubscription.enabled" :disabled="scriptStore.loading" @update:value="toggleSubscription(selectedSubscription.id, $event)">
                    <template #checked>{{ t('scripts.enabled') }}</template><template #unchecked>{{ t('scripts.disabled') }}</template>
                  </n-switch>
                  <n-button size="small" :loading="scriptStore.refreshingSubscriptionId === selectedSubscription.id" @click="refreshSubscription(selectedSubscription.id)"><template #icon><RefreshCw :size="14" /></template>{{ t('scripts.refresh') }}</n-button>
                  <n-popconfirm @positive-click="deleteSubscription(selectedSubscription.id)"><template #trigger><n-button size="small" type="error" secondary :title="t('scripts.deleteSubscription')" :aria-label="t('scripts.deleteSubscription')"><template #icon><Trash2 :size="14" /></template></n-button></template>{{ t('scripts.deleteSubscriptionConfirm') }}</n-popconfirm>
                </n-space>
              </div>
              <n-alert v-if="selectedSubscription.lastError" type="warning" :show-icon="false" style="margin-bottom: 12px">{{ selectedSubscription.lastError }}</n-alert>
              <n-empty v-if="!scriptStore.loading && filteredSubscriptionScripts.length === 0" :description="t('scripts.noCachedScripts')" />
              <div v-else class="script-list">
                <article v-for="script in filteredSubscriptionScripts" :key="`${script.subscriptionId}:${script.id}`" class="script-card">
                  <div class="script-card-heading"><div><h3>{{ script.name }}</h3><p>{{ script.description || selectedSubscription?.name }}</p></div><n-tag size="small" :type="tagType(script.riskLevel)">{{ script.riskLevel }}</n-tag></div>
                  <div class="tag-row"><n-tag size="small" :bordered="false">{{ selectedSubscription?.name }} · {{ script.version }}</n-tag><n-tag v-for="tag in script.tags" :key="tag" size="small" :bordered="false">{{ tag }}</n-tag></div>
                  <div class="script-actions"><n-button size="small" secondary @click="copyScript(script)"><template #icon><Copy :size="14" /></template>{{ t('scripts.copyToMine') }}</n-button></div>
                </article>
              </div>
            </template>
            <n-empty v-else :description="t('scripts.selectSubscription')" />
          </section>
        </div>
      </n-tab-pane>
    </n-tabs>

    <n-modal v-model:show="showScriptForm" preset="card" :title="editingScript ? t('scripts.editTitle') : t('scripts.newTitle')" style="width: min(680px, calc(100vw - 32px))">
      <n-alert v-if="formError" type="error" style="margin-bottom: 16px">{{ formError }}</n-alert>
      <n-form label-placement="top">
        <n-form-item :label="t('scripts.name')" required><n-input v-model:value="scriptForm.name" maxlength="80" show-count /></n-form-item>
        <n-form-item :label="t('scripts.descriptionLabel')"><n-input v-model:value="scriptForm.description" type="textarea" :rows="2" maxlength="500" show-count /></n-form-item>
        <n-form-item :label="t('scripts.tags')"><n-select v-model:value="scriptForm.tags" multiple filterable tag :max-tag-count="5" :placeholder="t('scripts.addTags')" /></n-form-item>
        <n-form-item :label="t('scripts.riskLevel')" required><n-select v-model:value="scriptForm.riskLevel" :options="riskOptions" /></n-form-item>
        <n-form-item :label="t('scripts.command')" required><n-input v-model:value="scriptForm.command" type="textarea" :rows="10" :autosize="{ minRows: 10, maxRows: 18 }" /></n-form-item>
      </n-form>
      <template #footer><n-space justify="end"><n-button @click="showScriptForm = false">{{ t('scripts.cancel') }}</n-button><n-button type="primary" :loading="scriptStore.loading" @click="saveScript">{{ t('scripts.save') }}</n-button></n-space></template>
    </n-modal>

    <n-modal v-model:show="showSubscriptionForm" preset="card" :title="t('scripts.addSubscriptionTitle')" style="width: min(560px, calc(100vw - 32px))">
      <n-alert v-if="formError" type="error" style="margin-bottom: 16px">{{ formError }}</n-alert>
      <n-form label-placement="top"><n-form-item :label="t('scripts.name')" required><n-input v-model:value="subscriptionForm.name" /></n-form-item><n-form-item :label="t('scripts.httpsUrl')" required><n-input v-model:value="subscriptionForm.url" placeholder="https://" /></n-form-item></n-form>
      <template #footer><n-space justify="end"><n-button @click="showSubscriptionForm = false">{{ t('scripts.cancel') }}</n-button><n-button type="primary" :loading="scriptStore.loading" @click="saveSubscription">{{ t('scripts.add') }}</n-button></n-space></template>
    </n-modal>

  </div>
</template>

<style scoped>
.scripts-view { min-width: 0; color: var(--app-text); }
.scripts-header, .subscription-detail, .script-card-heading, .script-actions, .tag-row { display: flex; align-items: center; }
.scripts-header { justify-content: space-between; gap: 16px; margin-bottom: 20px; }
.scripts-header h2 { margin: 0; font-size: 22px; font-weight: 600; }
.scripts-header p, .script-card p, .subscription-detail p { margin: 4px 0 0; color: var(--app-muted); font-size: 13px; }
.script-toolbar { max-width: 420px; margin: 0 0 16px; }
.script-list { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 10px; }
.script-card { border: 1px solid var(--app-border); background: var(--app-surface); border-radius: 6px; padding: 14px; min-width: 0; }
.script-card-heading { justify-content: space-between; align-items: flex-start; gap: 12px; }
.script-card h3, .subscription-detail h3 { margin: 0; font-size: 15px; font-weight: 600; }
.tag-row { flex-wrap: wrap; gap: 6px; margin-top: 12px; min-height: 22px; }
.script-actions { gap: 8px; margin-top: 14px; }
.subscriptions-layout { display: grid; grid-template-columns: 210px minmax(0, 1fr); gap: 20px; min-height: 380px; }
.subscription-list { border-right: 1px solid var(--app-border); padding-right: 12px; display: flex; flex-direction: column; gap: 5px; }
.subscription-item { text-align: left; border: 0; border-radius: 4px; background: transparent; color: var(--app-text); padding: 9px; cursor: pointer; }
.subscription-item:hover, .subscription-item.selected { background: var(--app-hover); }
.subscription-item strong, .subscription-item span { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.subscription-item span { color: var(--app-muted); font-size: 12px; margin-top: 2px; }
.subscription-content { min-width: 0; }
.subscription-detail { justify-content: space-between; align-items: flex-start; gap: 12px; margin-bottom: 14px; }
.subscription-detail p { overflow-wrap: anywhere; }
@media (max-width: 720px) { .subscriptions-layout { grid-template-columns: 1fr; } .subscription-list { border-right: 0; border-bottom: 1px solid var(--app-border); padding: 0 0 12px; flex-direction: row; overflow-x: auto; } .subscription-item { min-width: 160px; } .scripts-header, .subscription-detail { align-items: flex-start; flex-direction: column; } }
</style>
