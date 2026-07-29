<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Network, Plus } from '@lucide/vue'
import {
  NAlert,
  NButton,
  NEmpty,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NModal,
  NPopconfirm,
  NSelect,
  NSpace,
} from 'naive-ui'
import EntityCard from './EntityCard.vue'
import { useVaultStore } from '../stores/vault'
import type { CreateSocks5ProxyRequest, Socks5ProxyView, UpdateSocks5ProxyRequest } from '../types'
import { useLocale } from '../composables/useLocale'

type ProxyAuthType = 'none' | 'password'

type ProxyForm = {
  name: string
  host: string
  port: number
  auth_type: ProxyAuthType
  username: string
  password: string
}

const vaultStore = useVaultStore()
const { t } = useLocale()

const showForm = ref(false)
const editingProxy = ref<Socks5ProxyView | null>(null)
const formError = ref('')
const form = ref<ProxyForm>(emptyForm())

const authTypeOptions = computed(() => [
  { label: t('proxies.noAuth'), value: 'none' },
  { label: t('auth.password'), value: 'password' },
])

onMounted(() => {
  void vaultStore.loadProxies()
})

function emptyForm(): ProxyForm {
  return {
    name: '',
    host: '',
    port: 1080,
    auth_type: 'none',
    username: '',
    password: '',
  }
}

function openCreate() {
  editingProxy.value = null
  form.value = emptyForm()
  formError.value = ''
  showForm.value = true
}

function openEdit(proxy: Socks5ProxyView) {
  editingProxy.value = proxy
  form.value = {
    name: proxy.name,
    host: proxy.host,
    port: proxy.port,
    auth_type: proxy.auth_type,
    username: proxy.username ?? '',
    // Passwords are intentionally never returned to or populated in the form.
    password: '',
  }
  formError.value = ''
  showForm.value = true
}

function validateForm() {
  if (!form.value.name.trim() || !form.value.host.trim()) {
    formError.value = t('proxies.nameHostRequired')
    return false
  }

  if (!Number.isInteger(form.value.port) || form.value.port < 1 || form.value.port > 65535) {
    formError.value = t('proxies.portInvalid')
    return false
  }

  if (form.value.auth_type === 'password' && !editingProxy.value && !form.value.password) {
    formError.value = t('form.passwordRequired')
    return false
  }

  return true
}

function baseRequest() {
  return {
    name: form.value.name.trim(),
    host: form.value.host.trim(),
    port: form.value.port,
    auth_type: form.value.auth_type,
    username: form.value.auth_type === 'password' ? form.value.username : undefined,
  }
}

async function handleSubmit() {
  formError.value = ''
  if (!validateForm()) return

  if (editingProxy.value) {
    const request: UpdateSocks5ProxyRequest = baseRequest()
    if (form.value.auth_type === 'password' && form.value.password) {
      request.password = form.value.password
    }

    const result = await vaultStore.updateProxy(editingProxy.value.id, request)
    if (result) {
      showForm.value = false
    } else {
      formError.value = vaultStore.error || t('form.saveFailed')
    }
    return
  }

  const request: CreateSocks5ProxyRequest = {
    ...baseRequest(),
    password: form.value.auth_type === 'password' ? form.value.password : undefined,
  }
  const result = await vaultStore.createProxy(request)
  if (result) {
    showForm.value = false
  } else {
    formError.value = vaultStore.error || t('form.saveFailed')
  }
}

async function handleDelete(id: string) {
  await vaultStore.deleteProxy(id)
}

function authMethod(proxy: Socks5ProxyView) {
  return proxy.auth_type === 'password' ? t('auth.password') : t('proxies.noAuth')
}
</script>

<template>
  <div class="proxies-view">
    <div class="proxies-header">
      <h2>{{ t('proxies.title') }}</h2>
      <n-button type="primary" @click="openCreate">
        <template #icon><Plus :size="15" /></template>
        {{ t('proxies.add') }}
      </n-button>
    </div>

    <n-empty v-if="vaultStore.proxies.length === 0" :description="t('proxies.empty')" style="padding: 60px 0">
      <template #extra>
        <n-button type="primary" @click="openCreate">{{ t('proxies.createFirst') }}</n-button>
      </template>
    </n-empty>

    <div v-else class="proxies-grid">
      <EntityCard
        v-for="proxy in vaultStore.proxies"
        :key="proxy.id"
        :icon="Network"
        color="#3b82f6"
        :title="proxy.name"
        :subtitle="`${proxy.host}:${proxy.port}`"
        :metadata="authMethod(proxy)"
      >
        <template #actions>
          <n-button size="tiny" quaternary :title="t('proxies.edit')" :aria-label="t('proxies.edit')" @click="openEdit(proxy)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
            </svg>
          </n-button>
          <n-popconfirm @positive-click="handleDelete(proxy.id)">
            <template #trigger>
              <n-button size="tiny" quaternary type="error" :title="t('proxies.delete')" :aria-label="t('proxies.delete')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="3 6 5 6 21 6" />
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                </svg>
              </n-button>
            </template>
            {{ t('proxies.deleteConfirm', { name: proxy.name }) }}
          </n-popconfirm>
        </template>
      </EntityCard>
    </div>

    <n-modal v-model:show="showForm" :title="editingProxy ? t('proxies.editTitle') : t('proxies.createTitle')" preset="card" style="width: 520px">
      <n-alert v-if="formError" type="error" style="margin-bottom: 16px">
        {{ formError }}
      </n-alert>

      <n-form label-placement="left" label-width="90">
        <n-form-item :label="t('form.name')" required>
          <n-input v-model:value="form.name" placeholder="My proxy" />
        </n-form-item>
        <n-form-item :label="t('form.host')" required>
          <n-input v-model:value="form.host" placeholder="proxy.example.com" />
        </n-form-item>
        <n-form-item :label="t('form.port')" required>
          <n-input-number v-model:value="form.port" :min="1" :max="65535" style="width: 100%" />
        </n-form-item>
        <n-form-item :label="t('form.auth')">
          <n-select v-model:value="form.auth_type" :options="authTypeOptions" />
        </n-form-item>
        <template v-if="form.auth_type === 'password'">
          <n-form-item :label="t('form.username')">
            <n-input v-model:value="form.username" autocomplete="username" />
          </n-form-item>
          <n-form-item :label="t('form.password')" :required="!editingProxy">
            <n-input
              v-model:value="form.password"
              type="password"
              show-password-on="click"
              :placeholder="editingProxy ? t('proxies.passwordUnchanged') : t('form.passwordPlaceholder')"
              autocomplete="new-password"
            />
          </n-form-item>
        </template>
      </n-form>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showForm = false">{{ t('form.cancel') }}</n-button>
          <n-button type="primary" :loading="vaultStore.loading" @click="handleSubmit">
            {{ editingProxy ? t('form.save') : t('form.create') }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.proxies-view {
  min-width: 0;
}

.proxies-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.proxies-header h2 {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--app-text);
}

.proxies-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 8px;
}

:deep(.entity-actions .n-button) {
  width: 22px;
  height: 22px;
  padding: 0;
  color: var(--app-muted);
}

:deep(.entity-actions .n-button .n-button__icon) { margin: 0; }
:deep(.entity-actions .n-button:hover) { color: var(--app-text); }
:deep(.entity-actions .n-button--error-type:hover) { color: #ef4444; }

@media (max-width: 760px) {
  .proxies-grid { grid-template-columns: minmax(0, 1fr); }
}
</style>
