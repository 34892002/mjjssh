<script setup lang="ts">
import type { Component } from 'vue'

defineProps<{
  icon: Component
  color: string
  title: string
  subtitle: string
  metadata?: string
  invertTextHierarchy?: boolean
}>()
</script>

<template>
  <article class="entity-card">
    <div class="entity-icon" :style="{ '--entity-color': color }">
      <component :is="icon" :size="26" :stroke-width="1.8" />
    </div>
    <div class="entity-content" :class="{ 'invert-text-hierarchy': invertTextHierarchy }">
      <div class="entity-title-row">
        <div class="entity-title" :title="title">{{ title }}</div>
        <div v-if="$slots.actions" class="entity-actions">
          <slot name="actions" />
        </div>
      </div>
      <div class="entity-subtitle" :title="subtitle">{{ subtitle }}</div>
      <div v-if="metadata" class="entity-metadata" :title="metadata">{{ metadata }}</div>
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

.invert-text-hierarchy .entity-title-row { min-height: 16px; }

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

.entity-subtitle,
.entity-metadata {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  line-height: 16px;
  color: var(--app-muted);
}

.entity-metadata {
  color: color-mix(in srgb, var(--app-muted) 80%, transparent);
}

.invert-text-hierarchy { gap: 0; }
.invert-text-hierarchy .entity-title { font-size: 11px; font-weight: 500; color: var(--app-muted); }
.invert-text-hierarchy .entity-subtitle { font-size: 15px; font-weight: 650; line-height: 20px; color: var(--app-text); }

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
