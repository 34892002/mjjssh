<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { AlertTriangle, X } from '@lucide/vue'

type DialogKind = 'input' | 'confirm'

const props = withDefaults(defineProps<{
  show: boolean
  kind: DialogKind
  title: string
  message?: string
  initialValue?: string
  placeholder?: string
  confirmText?: string
  danger?: boolean
}>(), {
  message: '',
  initialValue: '',
  placeholder: '',
  confirmText: '确认',
  danger: false,
})
const emit = defineEmits<{ close: []; confirm: [value: string] }>()
const input = ref<HTMLInputElement | null>(null)
const value = ref('')

watch(() => props.show, async (show) => {
  if (!show) return
  value.value = props.initialValue
  await nextTick()
  input.value?.focus()
}, { immediate: true })

function confirm() {
  const result = value.value.trim()
  if (props.kind === 'input' && !result) return
  emit('confirm', result)
}
</script>

<template>
  <div v-if="show" class="action-backdrop" @click.self="emit('close')">
    <section class="action-dialog" role="dialog" aria-modal="true" :aria-label="title" @keydown.esc="emit('close')">
      <header class="action-title">
        <div class="action-heading"><AlertTriangle v-if="danger" :size="19" class="danger-icon" /><strong>{{ title }}</strong></div>
        <button title="关闭" @click="emit('close')"><X :size="17" /></button>
      </header>
      <p v-if="message" class="action-message">{{ message }}</p>
      <input v-if="kind === 'input'" ref="input" v-model="value" class="action-input" :placeholder="placeholder" @keydown.enter="confirm">
      <footer class="action-actions">
        <button @click="emit('close')">取消</button>
        <button :class="{ danger }" @click="confirm">{{ confirmText }}</button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.action-backdrop { position: fixed; inset: 0; z-index: 510; display: grid; place-items: center; background: rgba(15, 23, 42, .42); }
.action-dialog { width: min(400px, calc(100vw - 32px)); padding: 20px; border: 1px solid var(--app-border); border-radius: 8px; background: var(--app-panel); box-shadow: 0 12px 30px var(--app-shadow); color: var(--app-text); }
.action-title { display: flex; align-items: center; justify-content: space-between; gap: 16px; }.action-heading { display: flex; align-items: center; gap: 9px; }.action-title strong { font-size: 16px; }.action-title button { display: grid; place-items: center; padding: 2px; border: 0; background: transparent; color: var(--app-muted); cursor: pointer; }.danger-icon { color: #a16207; }
.action-message { margin: 14px 0 0; color: var(--app-text); font-size: 13px; line-height: 1.55; white-space: pre-wrap; word-break: break-word; }.action-input { width: 100%; height: 34px; margin-top: 16px; padding: 0 10px; border: 1px solid var(--app-border); border-radius: 5px; outline: 0; box-sizing: border-box; background: var(--app-code); color: var(--app-text); font: inherit; }.action-input:focus { border-color: var(--app-accent); }
.action-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 20px; }.action-actions button { height: 34px; padding: 0 15px; border: 1px solid var(--app-border); border-radius: 5px; background: transparent; color: var(--app-text); cursor: pointer; }.action-actions button:last-child { border-color: var(--app-accent); background: var(--app-accent); color: #fff; }.action-actions button.danger { border-color: #dc2626; background: #dc2626; color: #fff; }
</style>
