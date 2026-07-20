<script setup lang="ts">
import type { Component } from 'vue'

defineProps<{
  icon: Component
  color: string
  title: string
  subtitle: string
}>()
</script>

<template>
  <article class="entity-card">
    <div class="entity-icon" :style="{ '--entity-color': color }">
      <component :is="icon" :size="26" :stroke-width="1.8" />
    </div>
    <div class="entity-content">
      <div class="entity-title-row">
        <div class="entity-title" :title="title">{{ title }}</div>
        <div v-if="$slots.actions" class="entity-actions">
          <slot name="actions" />
        </div>
      </div>
      <div class="entity-subtitle" :title="subtitle">{{ subtitle }}</div>
    </div>
    <div v-if="$slots.footer" class="entity-footer">
      <slot name="footer" />
    </div>
  </article>
</template>

<style scoped>
.entity-card {
  display: grid;
  grid-template-columns: 50px minmax(0, 1fr);
  align-items: center;
  column-gap: 10px;
  row-gap: 7px;
  min-height: 76px;
  padding: 11px 12px;
  background: var(--app-surface);
  border: 1px solid var(--app-border);
  border-radius: 7px;
  transition: background-color .15s, border-color .15s;
}

.entity-card:hover {
  background: var(--app-elevated);
  border-color: color-mix(in srgb, var(--app-accent) 52%, var(--app-border));
}

.entity-icon {
  display: grid;
  width: 50px;
  height: 50px;
  place-items: center;
  border-radius: 8px;
  background: color-mix(in srgb, var(--entity-color) 18%, var(--app-surface));
  color: var(--entity-color);
}

.entity-content {
  min-width: 0;
  display: grid;
  gap: 3px;
}

.entity-title-row {
  display: flex;
  align-items: center;
  min-width: 0;
  min-height: 24px;
}

.entity-title {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 15px;
  font-weight: 650;
  color: var(--app-text);
}

.entity-subtitle {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  line-height: 16px;
  color: var(--app-muted);
}

.entity-actions {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 1px;
  margin-left: 4px;
  opacity: 0;
  transition: opacity .15s;
}

.entity-card:hover .entity-actions,
.entity-card:focus-within .entity-actions {
  opacity: 1;
}

.entity-footer {
  grid-column: 1 / -1;
  min-width: 0;
  padding-top: 5px;
  border-top: 1px solid color-mix(in srgb, var(--app-border) 68%, transparent);
}
</style>
