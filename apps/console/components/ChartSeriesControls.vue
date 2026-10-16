<script setup lang="ts">
defineProps<{
  options: readonly { key: string; label: string }[];
  selected: readonly string[];
  chartId: string;
}>();
const emit = defineEmits<{ toggle: [key: string]; all: [] }>();
</script>

<template>
  <div class="series-controls" role="group" aria-label="Compare chart series">
    <span class="series-hint">Click a card for one view, or compare:</span>
    <button
      v-for="option in options"
      :key="option.key"
      type="button"
      class="series-toggle"
      :class="{ 'is-selected': selected.includes(option.key) }"
      :aria-pressed="selected.includes(option.key)"
      :aria-disabled="selected.length === 1 && selected.includes(option.key)"
      :aria-controls="chartId"
      @click="emit('toggle', option.key)"
    >
      <span aria-hidden="true">{{
        selected.includes(option.key) ? "✓" : "+"
      }}</span>
      {{ option.label }}
    </button>
    <button
      type="button"
      class="series-all"
      :aria-controls="chartId"
      @click="emit('all')"
    >
      Show all
    </button>
    <span class="sr-only"
      >At least one series stays selected. Each toggle adds or removes a
      series.</span
    >
  </div>
</template>

<style scoped>
.series-controls {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin: -8px 0 24px;
}
.series-hint {
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-right: 4px;
}
.series-toggle,
.series-all {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--border-default);
  background: var(--bg-elevated);
  color: var(--text-secondary);
  border-radius: 999px;
  padding: 7px 12px;
  font: inherit;
  font-size: 0.75rem;
  cursor: pointer;
}
.series-toggle.is-selected {
  color: var(--brand-text-safe);
  border-color: var(--owleye-violet, #3d5afe);
  background: color-mix(
    in srgb,
    var(--owleye-violet, #3d5afe) 10%,
    var(--bg-elevated)
  );
}
.series-toggle > span {
  font-weight: 750;
}
.series-all {
  color: var(--brand-text-safe);
  text-decoration: underline;
  text-underline-offset: 3px;
  border-color: transparent;
}
.series-toggle:focus-visible,
.series-all:focus-visible {
  outline: 3px solid var(--owleye-violet, #3d5afe);
  outline-offset: 3px;
}
@media (max-width: 700px) {
  .series-hint {
    width: 100%;
  }
}
</style>
