<script setup lang="ts">
import type { BreakdownMetric } from "~/utils/breakdownMetric";
defineProps<{ modelValue: BreakdownMetric; label: string }>();
const emit = defineEmits<{ "update:modelValue": [value: BreakdownMetric] }>();
</script>
<template>
  <div class="view-switcher metric-switcher" :aria-label="label" role="group">
    <button
      type="button"
      :class="{ active: modelValue === 'pageviews' }"
      :aria-pressed="modelValue === 'pageviews'"
      @click="emit('update:modelValue', 'pageviews')"
    >
      Page views
    </button>
    <button
      type="button"
      :class="{ active: modelValue === 'visitors' }"
      :aria-pressed="modelValue === 'visitors'"
      @click="emit('update:modelValue', 'visitors')"
    >
      Unique visitors
    </button>
  </div>
</template>
<style scoped>
.metric-switcher {
  flex-wrap: wrap;
  max-width: 100%;
}
</style>
