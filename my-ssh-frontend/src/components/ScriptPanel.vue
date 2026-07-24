<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ChevronDown, ChevronRight, Code2, Folder } from '@lucide/vue'
import { NAlert, NEmpty, NSpin, NTag } from 'naive-ui'
import { useLocale } from '../composables/useLocale'
import { useScriptStore } from '../stores/scripts'
import type { ScriptRiskLevel, ScriptView, SubscriptionScriptView } from '../types'

interface PanelScript {
  id: string
  name: string
  command: string
  tags: string[]
  riskLevel: ScriptRiskLevel
  source: string
}

const emit = defineEmits<{
  insert: [command: string]
}>()

const scriptStore = useScriptStore()
const { language, t } = useLocale()
const expandedGroups = ref(new Set<string>())
const subscriptionScripts = ref<SubscriptionScriptView[]>([])

const subscriptionNames = computed(() => new Map(scriptStore.subscriptions.map((item) => [item.id, item.name])))
const scripts = computed<PanelScript[]>(() => [
  ...scriptStore.scripts.map((script: ScriptView) => ({
    ...script,
    source: t('scripts.panelMyScripts'),
  })),
  ...subscriptionScripts.value.map((script: SubscriptionScriptView) => ({
    ...script,
    source: subscriptionNames.value.get(script.subscriptionId) ?? t('scripts.panelSubscriptionScripts'),
  })),
])

const groups = computed(() => {
  const grouped = new Map<string, PanelScript[]>()
  for (const script of scripts.value) {
    const group = script.tags[0] || t('scripts.panelUncategorized')
    const entries = grouped.get(group) ?? []
    entries.push(script)
    grouped.set(group, entries)
  }
  return [...grouped.entries()].sort(([left], [right]) => left.localeCompare(right, language.value))
})

function toggleGroup(group: string) {
  const next = new Set(expandedGroups.value)
  if (next.has(group)) next.delete(group)
  else next.add(group)
  expandedGroups.value = next
}

function insertScript(script: PanelScript) {
  emit('insert', script.command.replace(/[\r\n]+$/, ''))
}

function tagType(level: ScriptRiskLevel) {
  return level === 'high' ? 'error' : level === 'medium' ? 'warning' : 'success'
}

onMounted(async () => {
  await Promise.all([
    scriptStore.loadScripts(),
    scriptStore.loadSubscriptions(),
  ])

  const enabledSubscriptions = scriptStore.subscriptions.filter((subscription) => subscription.enabled)
  const cachedResults = await Promise.all(enabledSubscriptions.map((subscription) =>
    invoke<SubscriptionScriptView[]>('list_cached_subscription_scripts', { id: subscription.id }).catch(() => []),
  ))
  subscriptionScripts.value = cachedResults.flat()
  expandedGroups.value = new Set(groups.value.map(([group]) => group))
})
</script>

<template>
  <section class="script-panel" @pointerdown.stop>
    <header class="script-panel-header">
      <strong>{{ t('scripts.title') }}</strong>
    </header>

    <n-alert v-if="scriptStore.error" type="error" :show-icon="false" class="script-error">
      {{ scriptStore.error }}
    </n-alert>

    <n-spin v-if="scriptStore.loading" size="small" class="script-loading" />
    <n-empty v-else-if="groups.length === 0" :description="t('scripts.panelEmpty')" class="script-empty" />
    <div v-else class="script-tree" role="tree" :aria-label="t('scripts.panelTree')">
      <div v-for="[group, entries] in groups" :key="group" class="script-group">
        <button class="group-button" :aria-expanded="expandedGroups.has(group)" @click="toggleGroup(group)">
          <ChevronDown v-if="expandedGroups.has(group)" :size="15" />
          <ChevronRight v-else :size="15" />
          <Folder :size="15" />
          <span>{{ group }}</span>
        </button>
        <div v-show="expandedGroups.has(group)" class="script-group-items">
          <button
            v-for="script in entries"
            :key="`${script.source}:${script.id}`"
            class="script-item"
            :title="t('scripts.panelInsertTitle')"
            @dblclick="insertScript(script)"
          >
            <Code2 :size="14" />
            <span>{{ script.name }}</span>
            <n-tag size="small" :type="tagType(script.riskLevel)" :bordered="false">{{ script.riskLevel }}</n-tag>
          </button>
        </div>
      </div>
    </div>

  </section>
</template>

<style scoped>
.script-panel { display: flex; flex-direction: column; width: 100%; max-height: calc(100vh - 88px); overflow: hidden; border: 1px solid var(--app-border); border-radius: 6px; background: var(--app-panel); box-shadow: 0 10px 24px var(--app-shadow); color: var(--app-text); }
.script-panel-header { display: flex; align-items: center; height: 38px; padding: 0 11px; border-bottom: 1px solid var(--app-border); font-size: 13px; }
.script-error { margin: 10px; }
.script-loading, .script-empty { display: flex; justify-content: center; padding: 28px 12px; }
.script-tree { min-height: 180px; max-height: min(420px, calc(100vh - 210px)); overflow: auto; padding: 7px 6px; }
.group-button, .script-item { display: flex; align-items: center; width: 100%; min-width: 0; border: 0; border-radius: 3px; background: transparent; color: var(--app-text); cursor: pointer; text-align: left; }
.group-button { gap: 5px; min-height: 29px; padding: 4px 6px; font-size: 12px; }
.group-button:hover, .script-item:hover { background: var(--app-hover); }
.script-group-items { padding-left: 20px; }
.script-item { gap: 6px; min-height: 29px; padding: 4px 6px; font-size: 12px; }
.script-item span { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.script-item :deep(.n-tag) { margin-left: auto; }
</style>
