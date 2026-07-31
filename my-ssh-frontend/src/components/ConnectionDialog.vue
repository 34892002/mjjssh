<script setup lang="ts">
import { ref, watch, type Component } from 'vue'
import { ChevronDown, ChevronRight, Fingerprint, KeyRound, Plug } from '@lucide/vue'
import { NButton } from 'naive-ui'
import { useLocale } from '../composables/useLocale'

type ConnectionStatus = 'connecting' | 'verifying' | 'authenticating' | 'success' | 'error' | 'host-key-confirm' | 'host-key-changed'
type StepState = 'pending' | 'active' | 'done' | 'error'
type ConnectionStep = { label: string; state: StepState; kind?: 'algorithms' }

const props = defineProps<{
  show: boolean
  host: string
  port: number
  username: string
  profileName: string
  proxy?: {
    name: string
    host: string
    port: number
  } | null
  icon: Component
  color: string
  status: ConnectionStatus
  error?: string
  hostKey?: {
    algorithm: string
    fingerprint: string
    expectedAlgorithm?: string
    expectedFingerprint?: string
  }
  negotiatedAlgorithms?: {
    kex: string
    host_key: string
    cipher: string
    client_mac: string
    server_mac: string
  }
  dark: boolean
}>()

const emit = defineEmits<{
  (e: 'retry'): void
  (e: 'trust-host-key'): void
  (e: 'close'): void
}>()

const { t } = useLocale()
const steps = ref<ConnectionStep[]>([])
const algorithmsExpanded = ref(false)

function proxyLabel() {
  return props.proxy ? `${props.proxy.name} (${props.proxy.host}:${props.proxy.port})` : ''
}

function fingerprintLabel() {
  return props.hostKey ? `${props.hostKey.algorithm}  ${props.hostKey.fingerprint}` : t('connection.verifyingFingerprint')
}

function algorithmSummary(value: string) {
  const normalized = value.toLowerCase()
  if (normalized.includes('mlkem')) return 'ML-KEM'
  if (normalized.includes('curve25519') || normalized.includes('x25519')) return 'X25519'
  if (normalized.includes('ed25519')) return 'Ed25519'
  if (normalized.includes('chacha20')) return 'ChaCha20'
  if (normalized.includes('aes')) return 'AES'
  return value
}

function negotiatedAlgorithmsSummary() {
  const algorithms = props.negotiatedAlgorithms
  if (!algorithms) return ''
  return t('connection.negotiatedAlgorithmsSummary', {
    kex: algorithmSummary(algorithms.kex),
    hostKey: algorithmSummary(algorithms.host_key),
    cipher: algorithmSummary(algorithms.cipher),
  })
}

function hasNegotiatedAlgorithms() {
  return Boolean(props.negotiatedAlgorithms)
}

function macLabel() {
  const algorithms = props.negotiatedAlgorithms
  if (!algorithms) return ''
  return algorithms.client_mac === algorithms.server_mac
    ? t('connection.negotiatedAlgorithmsMacBidirectional', { mac: algorithms.client_mac })
    : t('connection.negotiatedAlgorithmsMacDirectional', {
      clientMac: algorithms.client_mac,
      serverMac: algorithms.server_mac,
    })
}

watch([() => props.status, () => props.negotiatedAlgorithms], ([status]) => {
  const secureChannel = { label: t('connection.secureChannelEstablished', { host: props.host, port: props.port }), state: 'done' as StepState }
  const proxyTunnel = props.proxy
    ? { label: t('connection.proxyTunnelEstablished', { proxy: proxyLabel() }), state: 'done' as StepState }
    : null
  const connectionRoute = {
    label: props.proxy
      ? t('connection.connectingViaProxy', { proxy: proxyLabel(), host: props.host, port: props.port })
      : t('connection.connecting', { host: props.host, port: props.port }),
    state: 'active' as StepState,
  }

  if (status === 'connecting') {
    steps.value = [{ label: t('connection.initializingSecureChannel'), state: 'done' }, connectionRoute]
  } else if (status === 'verifying') {
    steps.value = [
      ...(proxyTunnel ? [proxyTunnel] : []),
      secureChannel,
      ...(hasNegotiatedAlgorithms() ? [{ label: negotiatedAlgorithmsSummary(), state: 'done' as StepState, kind: 'algorithms' as const }] : []),
      { label: fingerprintLabel(), state: 'active' },
    ]
  } else if (status === 'host-key-confirm') {
    steps.value = [...(proxyTunnel ? [proxyTunnel] : []), secureChannel, { label: t('connection.awaitingFingerprintConfirmation'), state: 'active' }]
  } else if (status === 'host-key-changed') {
    steps.value = [...(proxyTunnel ? [proxyTunnel] : []), secureChannel, { label: t('connection.hostKeyChanged'), state: 'error' }]
  } else if (status === 'authenticating') {
    steps.value = [
      ...(proxyTunnel ? [proxyTunnel] : []),
      secureChannel,
      ...(hasNegotiatedAlgorithms() ? [{ label: negotiatedAlgorithmsSummary(), state: 'done' as StepState, kind: 'algorithms' as const }] : []),
      { label: fingerprintLabel(), state: 'done' },
      { label: t('connection.authenticating'), state: 'active' },
    ]
  } else if (status === 'success') {
    steps.value = [
      ...(proxyTunnel ? [proxyTunnel] : []),
      secureChannel,
      ...(hasNegotiatedAlgorithms() ? [{ label: negotiatedAlgorithmsSummary(), state: 'done' as StepState, kind: 'algorithms' as const }] : []),
      { label: fingerprintLabel(), state: 'done' },
      { label: t('connection.authenticated'), state: 'done' },
    ]
  } else {
    steps.value = [...(props.proxy ? [connectionRoute] : []), { label: props.error || t('connection.failed'), state: 'error' }]
  }
}, { immediate: true })

</script>

<template>
  <div v-if="show" class="connection-surface">
    <section class="conn-dialog" :class="{ 'theme-dark': dark }" role="status" aria-live="polite">
      <div class="conn-header">
        <div class="conn-icon" :style="{ '--profile-color': color }">
          <component :is="icon" :size="24" :stroke-width="1.8" />
        </div>
        <div class="conn-info">
          <div class="conn-name">{{ profileName }}</div>
          <div class="conn-addr">SSH {{ username }}@{{ host }}:{{ port }}</div>
        </div>
      </div>

      <div class="connection-rail" :class="status" :aria-label="t('connection.progress')">
        <div class="rail-segment connection-segment" />
        <div class="rail-segment verification-segment" />
        <div class="rail-node connection-node"><Plug :size="15" /></div>
        <div class="rail-node fingerprint-node"><Fingerprint :size="15" /></div>
        <div class="rail-node authentication-node"><KeyRound :size="15" /></div>
      </div>

      <div class="conn-steps">
        <div v-for="(step, index) in steps" :key="index" class="step" :class="step.state">
          <span class="step-dot" />
          <template v-if="step.kind === 'algorithms' && negotiatedAlgorithms">
            <div class="algorithm-step">
              <button
                type="button"
                class="algorithm-toggle"
                :aria-expanded="algorithmsExpanded"
                @click="algorithmsExpanded = !algorithmsExpanded"
              >
                <ChevronDown v-if="algorithmsExpanded" :size="14" />
                <ChevronRight v-else :size="14" />
                <span>{{ step.label }}</span>
              </button>
              <dl v-if="algorithmsExpanded" class="algorithm-details">
                <div>
                  <dt>{{ t('connection.algorithmKex') }}</dt>
                  <dd>{{ negotiatedAlgorithms.kex }}</dd>
                </div>
                <div>
                  <dt>{{ t('connection.algorithmHostKey') }}</dt>
                  <dd>{{ negotiatedAlgorithms.host_key }}</dd>
                </div>
                <div>
                  <dt>{{ t('connection.algorithmCipher') }}</dt>
                  <dd>{{ negotiatedAlgorithms.cipher }}</dd>
                </div>
                <div>
                  <dt>{{ t('connection.algorithmMac') }}</dt>
                  <dd>{{ macLabel() }}</dd>
                </div>
              </dl>
            </div>
          </template>
          <span v-else class="step-label">{{ step.label }}</span>
        </div>
      </div>

      <div v-if="status === 'host-key-confirm'" class="host-key-confirmation">
        <p>{{ t('connection.firstConnectionWarning') }}</p>
        <code>{{ hostKey?.fingerprint }}</code>
      </div>
      <div v-else-if="status === 'host-key-changed'" class="host-key-warning">
        <p>{{ t('connection.hostKeyChangedWarning') }}</p>
        <code>{{ t('connection.savedFingerprint') }}: {{ hostKey?.expectedAlgorithm }} {{ hostKey?.expectedFingerprint }}</code>
        <code>{{ t('connection.currentFingerprint') }}: {{ hostKey?.algorithm }} {{ hostKey?.fingerprint }}</code>
      </div>

      <div class="conn-actions">
        <n-button v-if="status === 'host-key-confirm'" size="small" quaternary @click="emit('close')">{{ t('connection.cancel') }}</n-button>
        <n-button v-if="status === 'host-key-confirm'" size="small" type="primary" @click="emit('trust-host-key')">{{ t('connection.confirmAndTrust') }}</n-button>
        <n-button v-if="status === 'error' || status === 'host-key-changed'" size="small" quaternary @click="emit('close')">{{ t('connection.closeSession') }}</n-button>
        <n-button v-if="status === 'error'" size="small" type="primary" @click="emit('retry')">{{ t('connection.restart') }}</n-button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.connection-surface { position: absolute; inset: 0; z-index: 4; display: grid; place-items: center; padding: 24px; background: rgba(7, 11, 18, .28); }
.conn-dialog { --app-panel: #ffffff; --app-border: #dbe3ef; --app-shadow: rgba(15, 23, 42, .16); --app-text: #1e293b; --app-muted: #64748b; --app-hover: #e8eef7; --app-accent: #2563eb; background: var(--app-panel); border: 1px solid var(--app-border); border-radius: 12px; box-shadow: 0 12px 30px var(--app-shadow); color: var(--app-text); padding: 20px; width: 400px; max-width: 90vw; }
.conn-dialog.theme-dark { --app-panel: #151a25; --app-border: #344057; --app-shadow: rgba(0, 0, 0, .32); --app-text: #cdd6f4; --app-muted: #9aa8be; --app-hover: #252e3e; --app-accent: #89b4fa; }
.conn-header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.conn-icon { width: 40px; height: 40px; border-radius: 10px; background: color-mix(in srgb, var(--profile-color) 20%, var(--app-panel)); display: flex; align-items: center; justify-content: center; color: var(--profile-color); flex-shrink: 0; }
.conn-name { font-size: 15px; font-weight: 600; color: var(--app-text); }
.conn-addr { font-size: 12px; color: var(--app-muted); font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; }
.connection-rail { --rail-complete: var(--app-accent); position: relative; display: grid; grid-template-columns: 28px minmax(28px, 1fr) 28px minmax(28px, 1fr) 28px; align-items: center; gap: 10px; margin: 2px 0 18px; }
.rail-segment { height: 4px; border-radius: 999px; background: var(--app-hover); transition: background .2s ease; }
.rail-node { width: 28px; height: 28px; display: grid; place-items: center; border-radius: 7px; color: var(--app-muted); background: var(--app-hover); transition: color .2s ease, background .2s ease; }
.connection-node { grid-column: 1; grid-row: 1; }
.connection-segment { grid-column: 2; grid-row: 1; }
.fingerprint-node { grid-column: 3; grid-row: 1; }
.verification-segment { grid-column: 4; grid-row: 1; }
.authentication-node { grid-column: 5; grid-row: 1; }
.connection-rail.connecting .connection-node,
.connection-rail.verifying .connection-node,
.connection-rail.host-key-confirm .connection-node,
.connection-rail.host-key-changed .connection-node,
.connection-rail.authenticating .connection-node,
.connection-rail.success .connection-node { color: var(--app-accent); background: color-mix(in srgb, var(--app-accent) 18%, var(--app-panel)); }
.connection-rail.verifying .connection-segment,
.connection-rail.host-key-confirm .connection-segment,
.connection-rail.authenticating .connection-segment,
.connection-rail.success .connection-segment { background: var(--rail-complete); }
.connection-rail.verifying .fingerprint-node,
.connection-rail.host-key-confirm .fingerprint-node { color: var(--app-accent); background: color-mix(in srgb, var(--app-accent) 18%, var(--app-panel)); animation: node-pulse 1.2s ease-in-out infinite; }
.connection-rail.authenticating .fingerprint-node,
.connection-rail.success .fingerprint-node { color: #15803d; background: rgba(21, 128, 61, .14); }
.connection-rail.authenticating .verification-segment,
.connection-rail.success .verification-segment { background: var(--rail-complete); }
.connection-rail.authenticating .authentication-node { color: var(--app-accent); background: color-mix(in srgb, var(--app-accent) 18%, var(--app-panel)); animation: node-pulse 1.2s ease-in-out infinite; }
.connection-rail.success { --rail-complete: #15803d; }
.connection-rail.success .connection-node,
.connection-rail.success .authentication-node { color: #15803d; background: rgba(21, 128, 61, .14); }
.connection-rail.host-key-changed { --rail-complete: #dc2626; }
.connection-rail.host-key-changed .connection-segment { background: #dc2626; }
.connection-rail.host-key-changed .fingerprint-node { color: #dc2626; background: rgba(220, 38, 38, .14); }
.connection-rail.error .connection-node { color: var(--app-accent); background: color-mix(in srgb, var(--app-accent) 18%, var(--app-panel)); }
@keyframes node-pulse { 0%, 100% { opacity: 1; } 50% { opacity: .62; } }
.conn-steps { display: flex; flex-direction: column; gap: 8px; margin-bottom: 16px; }
.step { display: flex; align-items: flex-start; gap: 8px; font-size: 12px; line-height: 18px; min-width: 0; }
.step-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; margin-top: 6px; }
.step-label { min-width: 0; overflow-wrap: anywhere; word-break: break-word; }
.algorithm-step { min-width: 0; flex: 1; }
.algorithm-toggle { display: flex; align-items: center; gap: 4px; width: 100%; min-width: 0; padding: 0; border: 0; background: transparent; color: var(--app-muted); font: inherit; line-height: inherit; text-align: left; cursor: pointer; }
.algorithm-toggle:hover { color: var(--app-text); }
.algorithm-toggle:focus-visible { outline: 2px solid var(--app-accent); outline-offset: 2px; border-radius: 2px; }
.algorithm-toggle span { min-width: 0; overflow-wrap: anywhere; }
.algorithm-toggle svg { flex-shrink: 0; }
.algorithm-details { display: grid; gap: 5px; margin: 7px 0 0; padding: 8px 10px; border-left: 2px solid var(--app-border); background: color-mix(in srgb, var(--app-hover) 58%, transparent); }
.algorithm-details div { display: grid; grid-template-columns: 66px minmax(0, 1fr); gap: 8px; }
.algorithm-details dt { color: var(--app-muted); }
.algorithm-details dd { min-width: 0; margin: 0; overflow-wrap: anywhere; word-break: break-word; color: var(--app-text); font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; font-size: 11px; }
.step.pending .step-dot { background: var(--app-border); }.step.pending .step-label { color: var(--app-muted); }
.step.active .step-dot { background: var(--app-accent); animation: dot-pulse 1s ease-in-out infinite; }.step.active .step-label { color: var(--app-text); }
.step.done .step-dot { background: #15803d; }.step.done .step-label { color: var(--app-muted); }
.step.error .step-dot { background: #dc2626; }.step.error .step-label { color: #dc2626; font-weight: 500; }
@keyframes dot-pulse { 0%, 100% { opacity: 1; transform: scale(1); } 50% { opacity: .5; transform: scale(.8); } }
.host-key-confirmation, .host-key-warning { border: 1px solid var(--app-border); background: var(--app-hover); padding: 10px; margin: -4px 0 16px; font-size: 12px; color: var(--app-muted); }
.host-key-warning { border-color: rgba(220, 38, 38, .55); color: #dc2626; }
.host-key-confirmation p, .host-key-warning p { margin: 0 0 8px; line-height: 1.5; }
.host-key-confirmation code, .host-key-warning code { display: block; overflow-wrap: anywhere; font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; font-size: 11px; color: var(--app-text); }.host-key-warning code { color: inherit; margin-top: 4px; }
.conn-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
