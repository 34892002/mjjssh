import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type {
  CreateScriptRequest,
  CreateScriptSubscriptionRequest,
  ScriptSubscriptionView,
  ScriptView,
  SubscriptionScriptView,
  UpdateScriptRequest,
  UpdateScriptSubscriptionRequest,
} from '../types/scripts'

export const useScriptStore = defineStore('scripts', () => {
  const scripts = ref<ScriptView[]>([])
  const subscriptions = ref<ScriptSubscriptionView[]>([])
  const subscriptionScripts = ref<SubscriptionScriptView[]>([])
  const loading = ref(false)
  const refreshingSubscriptionId = ref<string | null>(null)
  const error = ref<string | null>(null)

  async function loadScripts() {
    loading.value = true
    error.value = null
    try {
      scripts.value = await invoke<ScriptView[]>('list_scripts')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function createScript(request: CreateScriptRequest): Promise<ScriptView | null> {
    loading.value = true
    error.value = null
    try {
      const script = await invoke<ScriptView>('create_script', { script: request })
      scripts.value.push(script)
      return script
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function updateScript(id: string, request: UpdateScriptRequest): Promise<ScriptView | null> {
    loading.value = true
    error.value = null
    try {
      const script = await invoke<ScriptView>('update_script', { id, script: request })
      const index = scripts.value.findIndex((item) => item.id === id)
      if (index >= 0) scripts.value.splice(index, 1, script)
      return script
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function deleteScript(id: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('delete_script', { id })
      scripts.value = scripts.value.filter((script) => script.id !== id)
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  async function loadSubscriptions() {
    loading.value = true
    error.value = null
    try {
      subscriptions.value = await invoke<ScriptSubscriptionView[]>('list_script_subscriptions')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function createSubscription(request: CreateScriptSubscriptionRequest): Promise<ScriptSubscriptionView | null> {
    loading.value = true
    error.value = null
    try {
      const subscription = await invoke<ScriptSubscriptionView>('add_script_subscription', { subscription: request })
      subscriptions.value.push(subscription)
      return subscription
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function updateSubscription(id: string, request: UpdateScriptSubscriptionRequest): Promise<ScriptSubscriptionView | null> {
    loading.value = true
    error.value = null
    try {
      const subscription = await invoke<ScriptSubscriptionView>('update_script_subscription', { id, subscription: request })
      const index = subscriptions.value.findIndex((item) => item.id === id)
      if (index >= 0) subscriptions.value.splice(index, 1, subscription)
      return subscription
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function deleteSubscription(id: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('remove_script_subscription', { id })
      subscriptions.value = subscriptions.value.filter((subscription) => subscription.id !== id)
      subscriptionScripts.value = subscriptionScripts.value.filter((script) => script.subscriptionId !== id)
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  async function refreshSubscription(id: string): Promise<boolean> {
    refreshingSubscriptionId.value = id
    error.value = null
    try {
      await invoke('refresh_script_subscription', { id })
      await loadSubscriptions()
      await loadSubscriptionScripts(id)
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      refreshingSubscriptionId.value = null
    }
  }

  async function loadSubscriptionScripts(subscriptionId: string) {
    loading.value = true
    error.value = null
    try {
      subscriptionScripts.value = await invoke<SubscriptionScriptView[]>('list_cached_subscription_scripts', { id: subscriptionId })
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  return {
    scripts,
    subscriptions,
    subscriptionScripts,
    loading,
    refreshingSubscriptionId,
    error,
    loadScripts,
    createScript,
    updateScript,
    deleteScript,
    loadSubscriptions,
    createSubscription,
    updateSubscription,
    deleteSubscription,
    refreshSubscription,
    loadSubscriptionScripts,
  }
})
