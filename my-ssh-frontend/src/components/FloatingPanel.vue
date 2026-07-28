<script setup lang="ts">
import { X } from '@lucide/vue'
import { NButton } from 'naive-ui'

withDefaults(defineProps<{
  show: boolean
  title: string
  width?: string
}>(), {
  width: '560px',
})

const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <div v-if="show" class="floating-panel-backdrop" @click.self="emit('close')">
    <section
      class="floating-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
      :style="{ '--floating-panel-width': width }"
      tabindex="-1"
      @keydown.esc="emit('close')"
    >
      <header class="floating-panel-header">
        <h4>{{ title }}</h4>
        <n-button quaternary circle size="small" title="关闭" aria-label="关闭" @click="emit('close')">
          <template #icon><X :size="17" /></template>
        </n-button>
      </header>
      <div class="floating-panel-content"><slot /></div>
    </section>
  </div>
</template>

<style scoped>
.floating-panel-backdrop { position: fixed; z-index: 1000; inset: 0; display: grid; place-items: center; padding: 28px; background: color-mix(in srgb, #000 48%, transparent); }
.floating-panel { display: grid; width: min(var(--floating-panel-width), 100%); max-height: min(620px, calc(100vh - 56px)); border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-surface); box-shadow: var(--app-shadow); color: var(--app-text); outline: 0; }
.floating-panel-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 16px 18px; border-bottom: 1px solid var(--app-border); }
.floating-panel-header h4 { margin: 0; font-size: 15px; }
.floating-panel-content { min-height: 0; overflow: auto; padding: 18px; }
@media (max-width: 620px) { .floating-panel-backdrop { padding: 16px; }.floating-panel { max-height: calc(100vh - 32px); }.floating-panel-header { padding: 14px 16px; }.floating-panel-content { padding: 16px; } }
</style>
