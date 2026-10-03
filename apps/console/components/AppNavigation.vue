<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
const props = defineProps<{
  modelValue: string;
  options: ConsoleSelectOption[];
}>();
const emit = defineEmits<{ change: [value: number | string] }>();

const createAppValue = "__create_app__";
const switcherOptions = computed<ConsoleSelectOption[]>(() => [
  ...props.options,
  { label: "+ Create app", value: createAppValue },
]);

function handleSelection(value: number | string) {
  if (value === createAppValue) {
    return navigateTo("/app/create");
  }
  emit("change", value);
}
</script>

<template>
  <div class="app-navigation" aria-label="App navigation">
    <div class="app-navigation-picker">
      <span id="workspace-app-label">Switch app</span>
      <ConsoleSelect
        labelledby="workspace-app-label"
        :model-value="modelValue"
        :options="switcherOptions"
        placeholder="Choose an app"
        @change="handleSelection"
      />
    </div>
    <CopyTrackingId v-if="options.some(option => option.value === modelValue)" :value="modelValue" />
  </div>
</template>

<style scoped>
.app-navigation {
  display: flex;
  align-items: end;
  gap: 16px;
  margin-bottom: 24px;
  padding-bottom: 20px;
  border-bottom: 1px solid var(--line, #d5d2c8);
}
.app-navigation-picker {
  display: grid;
  gap: 6px;
  width: min(420px, 100%);
  min-width: 0;
}
.app-navigation-picker > span {
  font-size: 0.75rem;
  font-weight: 700;
}
.app-navigation-picker :deep(.console-select-popover) {
  width: 100%;
}
.app-navigation-picker :deep(.console-select-option:last-child) {
  margin-top: 4px;
  border-top: 1px solid var(--line, #d5d2c8);
  border-radius: 0 0 8px 8px;
  color: var(--brand-text-safe);
}
@media (max-width: 600px) {
  .app-navigation {
    align-items: stretch;
    flex-direction: column;
  }
  .app-navigation-picker {
    width: 100%;
  }
}
</style>
