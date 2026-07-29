import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { translate } from '../composables/useLocale'
import type {
  SshProfileView,
  CreateProfileRequest,
  UpdateProfileRequest,
  SshKeyView,
  CreateKeyRequest,
  GenerateSshKeyRequest,
  GenerateSshKeyResult,
  Socks5ProxyView,
  CreateSocks5ProxyRequest,
  UpdateSocks5ProxyRequest,
} from '../types'

export const useVaultStore = defineStore('vault', () => {
  const isReady = ref(false)
  const profiles = ref<SshProfileView[]>([])
  const keysLoaded = ref(false)
  const proxiesLoaded = ref(false)
  const loading = ref(false)
  const error = ref<string | null>(null)

  /// 初始化 vault，并只加载主机首页需要的数据
  async function init() {
    error.value = null
    try {
      await invoke('init_vault')
      isReady.value = true
      await loadProfiles()
    } catch (e) {
      isReady.value = false
      profiles.value = []
      error.value = translate('vault.openFailed', { error: String(e) })
    }
  }


  async function loadProfiles() {
    loading.value = true
    try {
      profiles.value = await invoke<SshProfileView[]>('list_profiles')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function refreshAfterSync() {
    await loadProfiles()
    if (keysLoaded.value) {
      try {
        sshKeys.value = await invoke<SshKeyView[]>('list_keys')
      } catch (e) {
        error.value = String(e)
      }
    }
    if (proxiesLoaded.value) {
      try {
        proxies.value = await invoke<Socks5ProxyView[]>('list_proxies')
      } catch (e) {
        error.value = String(e)
      }
    }
  }


  async function createProfile(req: CreateProfileRequest): Promise<SshProfileView | null> {
    loading.value = true
    error.value = null
    try {
      const profile = await invoke<SshProfileView>('create_profile', { profile: req })
      const existingIndex = profiles.value.findIndex((item) => item.id === profile.id)
      if (existingIndex >= 0) {
        profiles.value.splice(existingIndex, 1, profile)
      } else {
        profiles.value.push(profile)
      }
      void loadProfiles()
      return profile
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function updateProfile(id: string, req: UpdateProfileRequest): Promise<SshProfileView | null> {
    loading.value = true
    error.value = null
    try {
      const profile = await invoke<SshProfileView>('update_profile', { id, profile: req })
      await loadProfiles()
      return profile
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function refreshProfileInfo(id: string): Promise<SshProfileView | null> {
    loading.value = true
    error.value = null
    try {
      const profile = await invoke<SshProfileView>('refresh_profile_info', { profileId: id })
      const index = profiles.value.findIndex((item) => item.id === id)
      if (index >= 0) profiles.value.splice(index, 1, profile)
      return profile
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function deleteProfile(id: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('delete_profile', { id })
      await loadProfiles()
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  // ==================== SSH Keys ====================

  const sshKeys = ref<SshKeyView[]>([])

  async function loadKeys() {
    if (keysLoaded.value) return
    try {
      sshKeys.value = await invoke<SshKeyView[]>('list_keys')
      keysLoaded.value = true
    } catch (e) {
      error.value = String(e)
    }
  }

  async function createKey(req: CreateKeyRequest): Promise<SshKeyView | null> {
    loading.value = true
    error.value = null
    try {
      const key = await invoke<SshKeyView>('create_key', { key: req })
      sshKeys.value.push(key)
      keysLoaded.value = true
      return key
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function generateSshKey(req: GenerateSshKeyRequest): Promise<GenerateSshKeyResult | null> {
    loading.value = true
    error.value = null
    try {
      const result = await invoke<GenerateSshKeyResult>('generate_ssh_key', { request: req })
      sshKeys.value.push(result.key)
      keysLoaded.value = true
      return result
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function deleteKey(id: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('delete_key', { id })
      sshKeys.value = sshKeys.value.filter((key) => key.id !== id)
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  // ==================== SOCKS5 Proxies ====================

  const proxies = ref<Socks5ProxyView[]>([])

  async function loadProxies() {
    if (proxiesLoaded.value) return
    try {
      proxies.value = await invoke<Socks5ProxyView[]>('list_proxies')
      proxiesLoaded.value = true
    } catch (e) {
      error.value = String(e)
    }
  }

  async function createProxy(req: CreateSocks5ProxyRequest): Promise<Socks5ProxyView | null> {
    loading.value = true
    error.value = null
    try {
      const proxy = await invoke<Socks5ProxyView>('create_proxy', { proxy: req })
      proxies.value.push(proxy)
      proxiesLoaded.value = true
      return proxy
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function updateProxy(id: string, req: UpdateSocks5ProxyRequest): Promise<Socks5ProxyView | null> {
    loading.value = true
    error.value = null
    try {
      const proxy = await invoke<Socks5ProxyView>('update_proxy', { id, proxy: req })
      const index = proxies.value.findIndex((existingProxy) => existingProxy.id === id)
      if (index !== -1) proxies.value.splice(index, 1, proxy)
      return proxy
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function deleteProxy(id: string): Promise<boolean> {
    loading.value = true
    error.value = null
    try {
      await invoke('delete_proxy', { id })
      proxies.value = proxies.value.filter((proxy) => proxy.id !== id)
      await loadProfiles()
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      loading.value = false
    }
  }

  async function updateKey(id: string, req: CreateKeyRequest): Promise<SshKeyView | null> {
    loading.value = true
    error.value = null
    try {
      const key = await invoke<SshKeyView>('update_key', { id, key: req })
      const index = sshKeys.value.findIndex((existingKey) => existingKey.id === id)
      if (index !== -1) sshKeys.value.splice(index, 1, key)
      return key
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  return {
    isReady,
    profiles,
    keysLoaded,
    proxiesLoaded,
    sshKeys,
    proxies,
    loading,
    error,
    init,
    loadProfiles,
    refreshAfterSync,
    createProfile,
    updateProfile,
    refreshProfileInfo,
    deleteProfile,
    loadKeys,
    createKey,
    generateSshKey,
    updateKey,
    deleteKey,
    loadProxies,
    createProxy,
    updateProxy,
    deleteProxy,
  }
})
