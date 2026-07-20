<script setup lang="ts">
import { onBeforeUnmount, ref, watch, type Component } from 'vue'
import { NModal, NButton, NSpace } from 'naive-ui'

const props = defineProps<{
  show: boolean
  host: string
  port: number
  username: string
  profileName: string
  icon: Component
  color: string
  status: 'connecting' | 'authenticating' | 'success' | 'error'
  error?: string
  dark: boolean
}>()

const emit = defineEmits<{
  (e: 'update:show', value: boolean): void
  (e: 'retry'): void
  (e: 'close'): void
}>()

const steps = ref<{ label: string; state: 'pending' | 'active' | 'done' | 'error' }[]>([])
let successCloseTimer: ReturnType<typeof setTimeout> | null = null

function clearSuccessCloseTimer() {
  if (!successCloseTimer) return
  clearTimeout(successCloseTimer)
  successCloseTimer = null
}

watch(() => props.status, (s) => {
  clearSuccessCloseTimer()
  if (s === 'connecting') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      { label: `正在连接 ${props.host}:${props.port}...`, state: 'active' },
      { label: 'SSH 认证...', state: 'pending' },
    ]
  } else if (s === 'authenticating') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      { label: `已连接 ${props.host}:${props.port}`, state: 'done' },
      { label: 'SSH 认证中...', state: 'active' },
    ]
  } else if (s === 'success') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      { label: `已连接 ${props.host}:${props.port}`, state: 'done' },
      { label: 'SSH 认证完成', state: 'done' },
    ]
    successCloseTimer = setTimeout(() => {
      successCloseTimer = null
      if (props.status === 'success') emit('update:show', false)
    }, 800)
  } else if (s === 'error') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      { label: `已连接 ${props.host}:${props.port}`, state: 'done' },
      { label: props.error || '连接失败', state: 'error' },
    ]
  }
}, { immediate: true })

watch(() => props.show, (show) => {
  if (!show) clearSuccessCloseTimer()
})

onBeforeUnmount(clearSuccessCloseTimer)
</script>

<template>
  <n-modal :show="show" @update:show="emit('update:show', $event)" :closable="status === 'error'" mask-closable>
    <div class="conn-dialog" :class="{ 'theme-dark': dark }">
      <div class="conn-header">
        <div class="conn-icon" :style="{ '--profile-color': color }">
          <component :is="icon" :size="24" :stroke-width="1.8" />
        </div>
        <div class="conn-info">
          <div class="conn-name">{{ profileName }}</div>
          <div class="conn-addr">SSH {{ username }}@{{ host }}:{{ port }}</div>
        </div>
      </div>

      <!-- Progress bar -->
      <div class="conn-progress">
        <div class="progress-bar" :class="status" />
      </div>

      <!-- Steps -->
      <div class="conn-steps">
        <div v-for="(step, i) in steps" :key="i" class="step" :class="step.state">
          <span class="step-dot" />
          <span class="step-label">{{ step.label }}</span>
        </div>
      </div>

      <!-- Actions -->
      <div class="conn-actions">
        <n-button v-if="status === 'error'" size="small" quaternary @click="emit('close')">关闭会话</n-button>
        <n-button v-if="status === 'error'" size="small" type="primary" @click="emit('retry')">
          重新开始
        </n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
.conn-dialog {
  --app-panel: #ffffff;
  --app-border: #dbe3ef;
  --app-shadow: rgba(15, 23, 42, .16);
  --app-text: #1e293b;
  --app-muted: #64748b;
  --app-hover: #e8eef7;
  --app-accent: #2563eb;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: 12px;
  box-shadow: 0 12px 30px var(--app-shadow);
  color: var(--app-text);
  padding: 20px;
  width: 400px;
  max-width: 90vw;
}

.conn-dialog.theme-dark {
  --app-panel: #151a25;
  --app-border: #344057;
  --app-shadow: rgba(0, 0, 0, .32);
  --app-text: #cdd6f4;
  --app-muted: #9aa8be;
  --app-hover: #252e3e;
  --app-accent: #89b4fa;
}

.conn-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.conn-icon {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  background: color-mix(in srgb, var(--profile-color) 20%, var(--app-panel));
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--profile-color);
  flex-shrink: 0;
}

.conn-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--app-text);
}

.conn-addr {
  font-size: 12px;
  color: var(--app-muted);
  font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace;
}

.conn-progress {
  height: 3px;
  background: var(--app-hover);
  border-radius: 2px;
  margin-bottom: 16px;
  overflow: hidden;
}

.progress-bar {
  height: 100%;
  border-radius: 2px;
  transition: width 0.3s ease;
}

.progress-bar.connecting,
.progress-bar.authenticating {
  width: 60%;
  background: var(--app-accent);
  animation: progress-pulse 1.5s ease-in-out infinite;
}

.progress-bar.success {
  width: 100%;
  background: #15803d;
}

.progress-bar.error {
  width: 100%;
  background: #dc2626;
}

@keyframes progress-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.6; }
}

.conn-steps {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 16px;
}

.step {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.step-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.step.pending .step-dot { background: var(--app-border); }
.step.pending .step-label { color: var(--app-muted); }

.step.active .step-dot { background: var(--app-accent); animation: dot-pulse 1s ease-in-out infinite; }
.step.active .step-label { color: var(--app-text); }

.step.done .step-dot { background: #15803d; }
.step.done .step-label { color: var(--app-muted); }

.step.error .step-dot { background: #dc2626; }
.step.error .step-label { color: #dc2626; font-weight: 500; background: rgba(220, 38, 38, 0.1); padding: 2px 6px; border-radius: 4px; }

@keyframes dot-pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.5; transform: scale(0.8); }
}

.conn-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
