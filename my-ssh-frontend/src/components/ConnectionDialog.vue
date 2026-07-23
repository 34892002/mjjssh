<script setup lang="ts">
import { onBeforeUnmount, ref, watch, type Component } from 'vue'
import { Fingerprint } from '@lucide/vue'
import { NModal, NButton } from 'naive-ui'

type ConnectionStatus = 'connecting' | 'verifying' | 'authenticating' | 'success' | 'error' | 'host-key-confirm' | 'host-key-changed'
type StepState = 'pending' | 'active' | 'done' | 'error'

const props = defineProps<{
  show: boolean
  host: string
  port: number
  username: string
  profileName: string
  icon: Component
  color: string
  status: ConnectionStatus
  error?: string
  hostKey?: { algorithm: string; fingerprint: string; expectedFingerprint?: string }
  dark: boolean
}>()

const emit = defineEmits<{
  (e: 'update:show', value: boolean): void
  (e: 'retry'): void
  (e: 'trust-host-key'): void
  (e: 'close'): void
}>()

const steps = ref<{ label: string; state: StepState; kind?: 'fingerprint' }[]>([])
let successCloseTimer: ReturnType<typeof setTimeout> | null = null

function clearSuccessCloseTimer() {
  if (!successCloseTimer) return
  clearTimeout(successCloseTimer)
  successCloseTimer = null
}

function fingerprintLabel() {
  return props.hostKey ? `${props.hostKey.algorithm}  ${props.hostKey.fingerprint}` : '正在验证主机指纹...'
}

watch(() => props.status, (status) => {
  clearSuccessCloseTimer()
  const connected = { label: `已连接 ${props.host}:${props.port}`, state: 'done' as StepState }
  const fingerprint = (state: StepState) => ({ label: fingerprintLabel(), state, kind: 'fingerprint' as const })

  if (status === 'connecting') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      { label: `正在连接 ${props.host}:${props.port}...`, state: 'active' },
      fingerprint('pending'),
      { label: 'SSH 认证...', state: 'pending' },
    ]
  } else if (status === 'verifying') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('active'),
      { label: 'SSH 认证...', state: 'pending' },
    ]
  } else if (status === 'host-key-confirm') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('active'),
      { label: '等待确认主机指纹', state: 'pending' },
    ]
  } else if (status === 'host-key-changed') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('error'),
      { label: '主机指纹已变化，连接已阻止', state: 'error' },
    ]
  } else if (status === 'authenticating') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('done'),
      { label: 'SSH 认证中...', state: 'active' },
    ]
  } else if (status === 'success') {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('done'),
      { label: 'SSH 认证完成', state: 'done' },
    ]
    successCloseTimer = setTimeout(() => {
      successCloseTimer = null
      if (props.status === 'success') emit('update:show', false)
    }, 800)
  } else {
    steps.value = [
      { label: '初始化安全通道...', state: 'done' },
      connected,
      fingerprint('done'),
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
  <n-modal :show="show" @update:show="emit('update:show', $event)" :closable="status === 'error' || status === 'host-key-changed'" mask-closable>
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

      <div class="conn-progress"><div class="progress-bar" :class="status" /></div>

      <div class="conn-steps">
        <div v-for="(step, index) in steps" :key="index" class="step" :class="[step.state, step.kind]">
          <Fingerprint v-if="step.kind === 'fingerprint'" class="step-fingerprint" :size="15" />
          <span v-else class="step-dot" />
          <span class="step-label">{{ step.label }}</span>
        </div>
      </div>

      <div v-if="status === 'host-key-confirm'" class="host-key-confirmation">
        <p>这是首次连接到此主机。请通过受信任渠道核对指纹后再继续。</p>
        <code>{{ hostKey?.fingerprint }}</code>
      </div>
      <div v-else-if="status === 'host-key-changed'" class="host-key-warning">
        <p>保存的指纹与服务器返回的指纹不同。这可能表示服务器重装，也可能表示连接正遭受拦截。</p>
        <code>已保存：{{ hostKey?.expectedFingerprint }}</code>
        <code>当前：{{ hostKey?.fingerprint }}</code>
      </div>

      <div class="conn-actions">
        <n-button v-if="status === 'host-key-confirm'" size="small" quaternary @click="emit('close')">取消</n-button>
        <n-button v-if="status === 'host-key-confirm'" size="small" type="primary" @click="emit('trust-host-key')">确认并信任</n-button>
        <n-button v-if="status === 'error' || status === 'host-key-changed'" size="small" quaternary @click="emit('close')">关闭会话</n-button>
        <n-button v-if="status === 'error'" size="small" type="primary" @click="emit('retry')">重新开始</n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
.conn-dialog { --app-panel: #ffffff; --app-border: #dbe3ef; --app-shadow: rgba(15, 23, 42, .16); --app-text: #1e293b; --app-muted: #64748b; --app-hover: #e8eef7; --app-accent: #2563eb; background: var(--app-panel); border: 1px solid var(--app-border); border-radius: 12px; box-shadow: 0 12px 30px var(--app-shadow); color: var(--app-text); padding: 20px; width: 400px; max-width: 90vw; }
.conn-dialog.theme-dark { --app-panel: #151a25; --app-border: #344057; --app-shadow: rgba(0, 0, 0, .32); --app-text: #cdd6f4; --app-muted: #9aa8be; --app-hover: #252e3e; --app-accent: #89b4fa; }
.conn-header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.conn-icon { width: 40px; height: 40px; border-radius: 10px; background: color-mix(in srgb, var(--profile-color) 20%, var(--app-panel)); display: flex; align-items: center; justify-content: center; color: var(--profile-color); flex-shrink: 0; }
.conn-name { font-size: 15px; font-weight: 600; color: var(--app-text); }
.conn-addr { font-size: 12px; color: var(--app-muted); font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; }
.conn-progress { height: 3px; background: var(--app-hover); border-radius: 2px; margin-bottom: 16px; overflow: hidden; }
.progress-bar { height: 100%; border-radius: 2px; transition: width .3s ease; }
.progress-bar.connecting, .progress-bar.verifying, .progress-bar.authenticating { width: 65%; background: var(--app-accent); animation: progress-pulse 1.5s ease-in-out infinite; }
.progress-bar.host-key-confirm { width: 72%; background: var(--app-accent); }
.progress-bar.success { width: 100%; background: #15803d; }
.progress-bar.error, .progress-bar.host-key-changed { width: 100%; background: #dc2626; }
@keyframes progress-pulse { 0%, 100% { opacity: 1; } 50% { opacity: .6; } }
.conn-steps { display: flex; flex-direction: column; gap: 8px; margin-bottom: 16px; }
.step { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.step-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.step.pending .step-dot { background: var(--app-border); }.step.pending .step-label { color: var(--app-muted); }
.step.active .step-dot { background: var(--app-accent); animation: dot-pulse 1s ease-in-out infinite; }.step.active .step-label { color: var(--app-text); }
.step.done .step-dot { background: #15803d; }.step.done .step-label { color: var(--app-muted); }
.step.error .step-dot { background: #dc2626; }.step.error .step-label { color: #dc2626; font-weight: 500; }
.step-fingerprint { color: var(--app-border); flex: 0 0 auto; }.step.active .step-fingerprint { color: var(--app-accent); animation: dot-pulse 1s ease-in-out infinite; }.step.done .step-fingerprint { color: #15803d; }.step.error .step-fingerprint { color: #dc2626; }
@keyframes dot-pulse { 0%, 100% { opacity: 1; transform: scale(1); } 50% { opacity: .5; transform: scale(.8); } }
.host-key-confirmation, .host-key-warning { border: 1px solid var(--app-border); background: var(--app-hover); padding: 10px; margin: -4px 0 16px; font-size: 12px; color: var(--app-muted); }
.host-key-warning { border-color: rgba(220, 38, 38, .55); color: #dc2626; }
.host-key-confirmation p, .host-key-warning p { margin: 0 0 8px; line-height: 1.5; }
.host-key-confirmation code, .host-key-warning code { display: block; overflow-wrap: anywhere; font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; font-size: 11px; color: var(--app-text); }.host-key-warning code { color: inherit; margin-top: 4px; }
.conn-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
