<script setup lang="ts">
import { onMounted, ref } from 'vue'
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
} from 'naive-ui'
import { useVaultStore } from '../stores/vault'
import type { SshKeyView, CreateKeyRequest } from '../types'

const vaultStore = useVaultStore()

const showForm = ref(false)
const editingKey = ref<SshKeyView | null>(null)
const form = ref<CreateKeyRequest>({
  name: '',
  key_type: 'key',
  private_key: '',
  cert_data: '',
})
const formError = ref('')

const keyTypeOptions = [
  { label: 'SSH 私钥', value: 'key' },
  { label: 'SSH 证书', value: 'certificate' },
]

onMounted(() => {
  void vaultStore.loadKeys()
})

function openCreate() {
  editingKey.value = null
  form.value = { name: '', key_type: 'key', private_key: '', cert_data: '' }
  formError.value = ''
  showForm.value = true
}

function openEdit(key: SshKeyView) {
  editingKey.value = key
  form.value = {
    name: key.name,
    key_type: key.key_type,
    private_key: '',
    cert_data: '',
  }
  formError.value = ''
  showForm.value = true
}

async function handleSubmit() {
  formError.value = ''

  if (!form.value.name) {
    formError.value = '请填写名称'
    return
  }

  // 新建时必须填私钥
  if (!editingKey.value && !form.value.private_key) {
    formError.value = '请填写私钥内容'
    return
  }

  // 证书类型必须填证书内容
  if (form.value.key_type === 'certificate' && !editingKey.value && !form.value.cert_data) {
    formError.value = '证书类型需要填写证书内容'
    return
  }

  // 编辑时如果没有填私钥，不传（保持原值）
  const data: CreateKeyRequest = {
    name: form.value.name,
    key_type: form.value.key_type,
    private_key: form.value.private_key || 'PLACEHOLDER',
    cert_data: form.value.cert_data || undefined,
  }

  // 如果是编辑且没填私钥，需要从后端读取原值
  // 简化处理：编辑时必须重新填私钥
  if (editingKey.value && !form.value.private_key) {
    formError.value = '编辑时需要重新填写私钥内容'
    return
  }

  if (editingKey.value) {
    // 编辑：调用后端更新
    const result = await vaultStore.updateKey(editingKey.value.id, data)
    if (result) {
      showForm.value = false
    } else {
      formError.value = vaultStore.error || '更新失败'
    }
  } else {
    // 新建
    const result = await vaultStore.createKey(data)
    if (result) {
      showForm.value = false
    } else {
      formError.value = vaultStore.error || '创建失败'
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
      <h2>密钥管理</h2>
      <n-button type="primary" size="small" @click="openCreate">
        <template #icon>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="12" y1="5" x2="12" y2="19"/>
            <line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
        </template>
        新增密钥
      </n-button>
    </div>

    <n-empty v-if="vaultStore.sshKeys.length === 0" description="暂无密钥" style="padding: 60px 0">
      <template #extra>
        <n-button type="primary" @click="openCreate">创建第一个密钥</n-button>
      </template>
    </n-empty>

    <div v-else class="keys-grid">
      <div
        v-for="key in vaultStore.sshKeys"
        :key="key.id"
        class="key-card"
      >
        <div class="key-icon" :class="key.key_type">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>
          </svg>
        </div>
        <div class="key-info">
          <div class="key-name">{{ key.name }}</div>
          <div class="key-type">{{ key.key_type === 'key' ? 'SSH 私钥' : 'SSH 证书' }}</div>
        </div>
        <div class="key-actions">
          <n-button class="key-action-edit" size="tiny" quaternary @click="openEdit(key)">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
            </svg>
          </n-button>
          <n-popconfirm @positive-click="handleDelete(key.id)">
            <template #trigger>
              <n-button class="key-action-delete" size="tiny" quaternary type="error">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="3 6 5 6 21 6"/>
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                </svg>
              </n-button>
            </template>
            确定删除 "{{ key.name }}"？
          </n-popconfirm>
        </div>
      </div>
    </div>

    <!-- Add/edit key modal -->
    <n-modal v-model:show="showForm" :title="editingKey ? '编辑密钥' : '新增密钥'" preset="card" style="width: 520px">
      <n-alert v-if="formError" type="error" style="margin-bottom: 16px">
        {{ formError }}
      </n-alert>

      <n-form label-placement="left" label-width="80">
        <n-form-item label="名称" required>
          <n-input v-model:value="form.name" placeholder="我的密钥" />
        </n-form-item>
        <n-form-item label="类型">
          <n-select v-model:value="form.key_type" :options="keyTypeOptions" :disabled="!!editingKey" />
        </n-form-item>
        <n-form-item label="私钥内容" :required="!editingKey">
          <n-input
            v-model:value="form.private_key"
            type="textarea"
            :placeholder="editingKey ? '留空则保持原密钥不变' : '粘贴 OpenSSH 私钥内容 (-----BEGIN ... PRIVATE KEY-----)'"
            :rows="8"
          />
        </n-form-item>
        <n-form-item v-if="form.key_type === 'certificate'" label="证书内容" :required="!editingKey">
          <n-input
            v-model:value="form.cert_data"
            type="textarea"
            :placeholder="editingKey ? '留空则保持原证书不变' : '粘贴 SSH 证书内容 (-----BEGIN SSH CERTIFICATE-----)'"
            :rows="6"
          />
        </n-form-item>
      </n-form>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showForm = false">取消</n-button>
          <n-button type="primary" :loading="vaultStore.loading" @click="handleSubmit">
            {{ editingKey ? '保存' : '创建' }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.keys-view {
  padding: 24px 32px;
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
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
}

.key-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  background: var(--app-surface);
  border: 1px solid var(--app-border);
  border-radius: 12px;
  transition: all 0.2s;
}

.key-card:hover {
  background: var(--app-elevated);
  border-color: var(--app-border);
}

.key-icon {
  width: 42px;
  height: 42px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: #fff;
}

.key-icon.key {
  background: linear-gradient(135deg, #f9e2af, #fab387);
}

.key-icon.certificate {
  background: linear-gradient(135deg, #a6e3a1, #94e2d5);
}

.key-info {
  flex: 1;
  min-width: 0;
}

.key-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--app-text);
  margin-bottom: 2px;
}

.key-type {
  font-size: 12px;
  color: var(--app-muted);
}

.key-actions {
  display: flex;
  gap: 2px;
  opacity: 0.4;
  transition: opacity 0.15s;
}

.key-card:hover .key-actions {
  opacity: 1;
}

.key-action-edit { color: var(--app-muted); }
.key-action-edit:hover { color: var(--app-text); }
.key-action-delete { color: #d9485f; }
</style>
