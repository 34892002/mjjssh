<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Copy, KeyRound, Plus, WandSparkles } from '@lucide/vue'
import EntityCard from './EntityCard.vue'
import {
  NButton,
  NEmpty,
  NPopconfirm,
  NSpace,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NAlert,
  NRadioButton,
  NRadioGroup,
} from 'naive-ui'
import { useVaultStore } from '../stores/vault'
import type { SshKeyView, CreateKeyRequest, GenerateSshKeyRequest, ImportedSshKeyAlgorithm } from '../types'
import { useLocale } from '../composables/useLocale'

const vaultStore = useVaultStore()
const { t } = useLocale()

const showForm = ref(false)
const showGenerator = ref(false)
const generatedPublicKey = ref('')
const generateForm = ref<GenerateSshKeyRequest>({
  name: '',
  algorithm: 'ed25519',
})
const generateError = ref('')
const publicKeyCopied = ref(false)
const editingKey = ref<SshKeyView | null>(null)
const form = ref<CreateKeyRequest>({
  name: '',
  key_type: 'key',
  algorithm: 'auto',
  private_key: '',
  cert_data: '',
})
const formError = ref('')

const keyTypeOptions = computed(() => [
  { label: t('keys.privateKey'), value: 'key' },
  { label: t('keys.certificate'), value: 'certificate' },
])

const importedAlgorithmOptions = computed(() => [
  { label: t('keys.autoDetect'), value: 'auto' },
  { label: 'RSA', value: 'ssh-rsa' },
  { label: 'Ed25519', value: 'ssh-ed25519' },
  { label: t('keys.dsaLegacy'), value: 'ssh-dss' },
])

onMounted(() => {
  void vaultStore.loadKeys()
})

function openCreate() {
  editingKey.value = null
  form.value = { name: '', key_type: 'key', algorithm: 'auto', private_key: '', cert_data: '' }
  formError.value = ''
  showForm.value = true
}

function openGenerate() {
  generateForm.value = { name: '', algorithm: 'ed25519' }
  generatedPublicKey.value = ''
  generateError.value = ''
  publicKeyCopied.value = false
  showGenerator.value = true
}

async function handleGenerate() {
  generateError.value = ''
  if (!generateForm.value.name.trim()) {
    generateError.value = t('keys.nameRequired')
    return
  }

  const result = await vaultStore.generateSshKey({
    ...generateForm.value,
    name: generateForm.value.name.trim(),
  })
  if (!result) {
    generateError.value = vaultStore.error || t('keys.generateFailed')
    return
  }

  generatedPublicKey.value = result.publicKey
}

async function copyPublicKey() {
  try {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager')
    await writeText(generatedPublicKey.value)
    publicKeyCopied.value = true
  } catch {
    generateError.value = t('keys.copyFailed')
  }
}

function openEdit(key: SshKeyView) {
  editingKey.value = key
  form.value = {
    name: key.name,
    key_type: key.key_type,
    algorithm: (key.algorithm || 'auto') as ImportedSshKeyAlgorithm,
    private_key: '',
    cert_data: '',
  }
  formError.value = ''
  showForm.value = true
}

async function handleSubmit() {
  formError.value = ''

  if (!form.value.name) {
    formError.value = t('keys.nameRequired')
    return
  }

  // 新建时必须填私钥
  if (!editingKey.value && !form.value.private_key) {
    formError.value = t('keys.privateKeyRequired')
    return
  }

  // 证书类型必须填证书内容
  if (form.value.key_type === 'certificate' && !editingKey.value && !form.value.cert_data) {
    formError.value = t('keys.certificateContentRequired')
    return
  }

  // 编辑时如果没有填私钥，不传（保持原值）
  const data: CreateKeyRequest = {
    name: form.value.name,
    key_type: form.value.key_type,
    algorithm: form.value.algorithm,
    private_key: form.value.private_key || 'PLACEHOLDER',
    cert_data: form.value.cert_data || undefined,
  }

  // 如果是编辑且没填私钥，需要从后端读取原值
  // 简化处理：编辑时必须重新填私钥
  if (editingKey.value && !form.value.private_key) {
    formError.value = t('keys.privateKeyRequiredForEdit')
    return
  }

  if (editingKey.value) {
    // 编辑：调用后端更新
    const result = await vaultStore.updateKey(editingKey.value.id, data)
    if (result) {
      showForm.value = false
    } else {
      formError.value = vaultStore.error || t('keys.updateFailed')
    }
  } else {
    // 新建
    const result = await vaultStore.createKey(data)
    if (result) {
      showForm.value = false
    } else {
      formError.value = vaultStore.error || t('keys.createFailed')
    }
  }
}

async function handleDelete(id: string) {
  await vaultStore.deleteKey(id)
}
</script>

<template>
  <div class="keys-view">
    <div class="keys-header">
      <h2>{{ t('keys.title') }}</h2>
      <n-space>
        <n-button @click="openGenerate">
          <template #icon><WandSparkles :size="15" /></template>
          {{ t('keys.generate') }}
        </n-button>
        <n-button type="primary" @click="openCreate">
          <template #icon><Plus :size="15" /></template>
          {{ t('keys.add') }}
        </n-button>
      </n-space>
    </div>

    <n-empty v-if="vaultStore.sshKeys.length === 0" :description="t('keys.empty')" style="padding: 60px 0">
      <template #extra>
        <n-button type="primary" @click="openCreate">{{ t('keys.createFirst') }}</n-button>
      </template>
    </n-empty>

    <div v-else class="keys-grid">
      <EntityCard
        v-for="key in vaultStore.sshKeys"
        :key="key.id"
        :icon="KeyRound"
        :color="key.key_type === 'key' ? '#f59e0b' : '#22c55e'"
        :title="key.name"
        :subtitle="key.algorithm || t('keys.unknownAlgorithm')"
        :metadata="key.key_type === 'key' ? t('keys.privateKey') : t('keys.certificate')"
      >
        <template #actions>
          <n-button size="tiny" quaternary :title="t('keys.edit')" :aria-label="t('keys.edit')" @click="openEdit(key)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
            </svg>
          </n-button>
          <n-popconfirm @positive-click="handleDelete(key.id)">
            <template #trigger>
              <n-button size="tiny" quaternary type="error" :title="t('keys.delete')" :aria-label="t('keys.delete')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="3 6 5 6 21 6"/>
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2 2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                </svg>
              </n-button>
            </template>
            {{ t('keys.deleteConfirm', { name: key.name }) }}
          </n-popconfirm>
        </template>
      </EntityCard>
    </div>

    <n-modal v-model:show="showGenerator" :title="t('keys.generateTitle')" preset="card" style="width: 520px">
      <n-alert v-if="generateError" type="error" style="margin-bottom: 16px">
        {{ generateError }}
      </n-alert>

      <n-form v-if="!generatedPublicKey" label-placement="left" label-width="80">
        <n-form-item :label="t('keys.name')" required>
          <n-input v-model:value="generateForm.name" :placeholder="t('keys.namePlaceholder')" @keyup.enter="handleGenerate" />
        </n-form-item>
        <n-form-item :label="t('keys.algorithm')">
          <n-radio-group v-model:value="generateForm.algorithm">
            <n-radio-button value="ed25519">Ed25519</n-radio-button>
            <n-radio-button value="rsa">RSA-4096</n-radio-button>
          </n-radio-group>
        </n-form-item>
        <n-alert type="info" :show-icon="false">
          {{ generateForm.algorithm === 'ed25519' ? t('keys.ed25519Description') : t('keys.rsaDescription') }}
        </n-alert>
      </n-form>

      <div v-else>
        <n-alert type="success" style="margin-bottom: 16px">{{ t('keys.generatedDescription') }}</n-alert>
        <n-form label-placement="top">
          <n-form-item :label="t('keys.publicKey')">
            <n-input :value="generatedPublicKey" type="textarea" :rows="4" readonly />
          </n-form-item>
        </n-form>
      </div>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showGenerator = false">{{ generatedPublicKey ? t('keys.done') : t('form.cancel') }}</n-button>
          <n-button v-if="generatedPublicKey" type="primary" @click="copyPublicKey">
            <template #icon><Copy :size="15" /></template>
            {{ publicKeyCopied ? t('keys.copied') : t('keys.copyPublicKey') }}
          </n-button>
          <n-button v-else type="primary" :loading="vaultStore.loading" @click="handleGenerate">
            {{ t('keys.generate') }}
          </n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- Add/edit key modal -->
    <n-modal v-model:show="showForm" :title="editingKey ? t('keys.editTitle') : t('keys.addTitle')" preset="card" style="width: 520px">
      <n-alert v-if="formError" type="error" style="margin-bottom: 16px">
        {{ formError }}
      </n-alert>

      <n-form label-placement="left" label-width="80">
        <n-form-item :label="t('keys.name')" required>
          <n-input v-model:value="form.name" :placeholder="t('keys.namePlaceholder')" />
        </n-form-item>
        <n-form-item :label="t('keys.type')">
          <n-select v-model:value="form.key_type" :options="keyTypeOptions" :disabled="!!editingKey" />
        </n-form-item>
        <n-form-item :label="t('keys.algorithm')">
          <n-select v-model:value="form.algorithm" :options="importedAlgorithmOptions" />
          <template #feedback>{{ t('keys.algorithmHint') }}</template>
        </n-form-item>
        <n-form-item :label="t('keys.privateKeyContent')" :required="!editingKey">
          <n-input
            v-model:value="form.private_key"
            type="textarea"
            :placeholder="editingKey ? t('keys.privateKeyUnchangedPlaceholder') : t('keys.privateKeyPlaceholder')"
            :rows="8"
          />
        </n-form-item>
        <n-form-item v-if="form.key_type === 'certificate'" :label="t('keys.certificateContent')" :required="!editingKey">
          <n-input
            v-model:value="form.cert_data"
            type="textarea"
            :placeholder="editingKey ? t('keys.certificateUnchangedPlaceholder') : t('keys.certificatePlaceholder')"
            :rows="6"
          />
        </n-form-item>
      </n-form>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showForm = false">{{ t('form.cancel') }}</n-button>
          <n-button type="primary" :loading="vaultStore.loading" @click="handleSubmit">
            {{ editingKey ? t('keys.save') : t('keys.create') }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.keys-view {
  min-width: 0;
}

.keys-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.keys-header h2 {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--app-text);
}

.keys-grid {
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
  .keys-grid { grid-template-columns: minmax(0, 1fr); }
}
</style>
