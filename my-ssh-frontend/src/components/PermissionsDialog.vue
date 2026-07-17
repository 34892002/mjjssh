<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { X } from '@lucide/vue'

const props = defineProps<{ show: boolean; name: string; mode: number }>()
const emit = defineEmits<{ close: []; apply: [mode: number] }>()
const bits = ref<boolean[]>([])

function bitsFromMode(mode: number) {
  return [8, 7, 6, 5, 4, 3, 2, 1, 0].map((shift) => Boolean(mode & (1 << shift)))
}
function toMode() {
  return [0, 3, 6].reduce((mode, offset) => {
    const digit = (bits.value[offset] ? 4 : 0) + (bits.value[offset + 1] ? 2 : 0) + (bits.value[offset + 2] ? 1 : 0)
    return (mode << 3) | digit
  }, 0)
}
const mode = computed(toMode)
const symbol = computed(() => bits.value.map((set, index) => set ? ['r', 'w', 'x'][index % 3] : '-').join(''))
watch(() => [props.show, props.mode], () => { if (props.show) bits.value = bitsFromMode(props.mode) }, { immediate: true })
</script>

<template>
  <div v-if="show" class="permission-backdrop" @click.self="emit('close')">
    <section class="permission-dialog" role="dialog" aria-modal="true" aria-label="编辑权限">
      <header class="permission-title"><div><strong>编辑权限</strong><span>{{ name }}</span></div><button title="关闭" @click="emit('close')"><X :size="17" /></button></header>
      <div v-for="(label, row) in ['所有者', '群组', '其他']" :key="label" class="permission-row"><span>{{ label }}</span><label v-for="(action, column) in ['R', 'W', 'X']" :key="action"><input v-model="bits[row * 3 + column]" type="checkbox"><span>{{ action }}</span></label></div>
      <div class="permission-summary"><span>八进制: {{ mode.toString(8) }}</span><span>符号: {{ symbol }}</span></div>
      <footer class="permission-actions"><button @click="emit('close')">取消</button><button class="primary" @click="emit('apply', mode)">应用</button></footer>
    </section>
  </div>
</template>

<style scoped>
.permission-backdrop { position: fixed; inset: 0; z-index: 500; display: grid; place-items: center; background: rgba(15, 23, 42, .42); }
.permission-dialog { width: 400px; padding: 22px; border: 1px solid var(--app-border); border-radius: 10px; background: var(--app-panel); box-shadow: 0 12px 30px var(--app-shadow); color: var(--app-text); }
.permission-title { display: flex; align-items: flex-start; justify-content: space-between; margin-bottom: 20px; }.permission-title strong { display: block; font-size: 17px; }.permission-title span { display: block; margin-top: 4px; color: var(--app-muted); font-size: 13px; }.permission-title button { display: grid; place-items: center; border: 0; background: transparent; color: var(--app-muted); cursor: pointer; }
.permission-row { display: grid; grid-template-columns: 84px repeat(3, 46px); align-items: center; min-height: 34px; }.permission-row > span { color: var(--app-text); }.permission-row label { display: flex; align-items: center; gap: 5px; color: var(--app-muted); font-size: 12px; }.permission-row input { accent-color: var(--app-accent); }
.permission-summary { display: flex; justify-content: space-between; margin-top: 14px; padding-top: 12px; border-top: 1px solid var(--app-border); color: var(--app-muted); font-family: monospace; font-size: 12px; }.permission-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 22px; }.permission-actions button { height: 34px; padding: 0 15px; border: 1px solid var(--app-border); border-radius: 7px; background: transparent; color: var(--app-text); cursor: pointer; }.permission-actions .primary { border-color: var(--app-accent); background: var(--app-accent); color: #fff; }
</style>
