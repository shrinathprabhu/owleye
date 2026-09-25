<script setup lang="ts">
import {
  analyticsDateBounds,
  analyticsInterval,
  customAnalyticsRange,
  type AnalyticsRange,
} from "~/utils/analyticsRange";

const emit = defineEmits<{ change: [range: AnalyticsRange] }>();
const selection = ref<number | string>(30);
const today = new Date().toISOString().slice(0, 10);
const start = ref(
  new Date(Date.parse(`${today}T00:00:00Z`) - 29 * 86_400_000)
    .toISOString()
    .slice(0, 10),
);
const end = ref(today);
const { min: earliest } = analyticsDateBounds(today);
const startMax = computed(() =>
  end.value >= earliest && end.value <= today ? end.value : today,
);
const endMin = computed(() =>
  start.value >= earliest && start.value <= today ? start.value : earliest,
);
const error = ref("");
const customRange = computed(() =>
  customAnalyticsRange(start.value, end.value, today),
);
const grouping = computed(() => {
  const days = customRange.value?.days;
  return days
    ? `${days} days · ${{ day: "Daily", week: "Weekly", month: "Monthly" }[analyticsInterval(days)]} chart`
    : "Choose dates within the last 365 days (including today).";
});
const options = [
  { label: "7 days", value: 7, description: "Daily detail" },
  { label: "30 days", value: 30, description: "Daily detail" },
  { label: "90 days", value: 90, description: "Weekly detail" },
  { label: "180 days", value: 180, description: "Weekly detail" },
  { label: "1 year", value: 365, description: "Monthly detail" },
  {
    label: "Custom dates",
    value: "custom",
    description: "Choose a start and end date",
  },
];
function select(value: number | string) {
  selection.value = value;
  error.value = "";
  if (value !== "custom") emit("change", { days: Number(value) });
}
function apply() {
  if (!customRange.value) {
    error.value =
      "Choose dates in order within the last 365 days (including today).";
    return;
  }
  error.value = "";
  emit("change", customRange.value);
}
</script>

<template>
  <div class="analytics-range-control">
    <div class="site-control">
      <span id="range-select-label">Range</span>
      <ConsoleSelect
        labelledby="range-select-label"
        :model-value="selection"
        :options="options"
        @change="select"
      />
    </div>
    <div v-if="selection === 'custom'" class="custom-dates">
      <label
        >From (UTC)<input
          v-model="start"
          type="date"
          :min="earliest"
          :max="startMax"
          required
          @keydown.enter.prevent="apply"
      /></label>
      <label
        >Through (UTC)<input
          v-model="end"
          type="date"
          :min="endMin"
          :max="today"
          required
          @keydown.enter.prevent="apply"
      /></label>
      <button
        class="button secondary compact"
        type="button"
        :disabled="!customRange"
        @click="apply"
      >
        Apply dates
      </button>
      <small class="date-help" aria-live="polite">{{ grouping }}</small>
      <p v-if="error" class="date-error" role="alert">{{ error }}</p>
    </div>
  </div>
</template>

<style scoped>
.analytics-range-control {
  flex: 1 1 240px;
  min-width: 0;
}
.custom-dates {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 12px;
  margin-top: 12px;
}
.custom-dates label {
  display: grid;
  gap: 6px;
  flex: 1 1 150px;
  min-width: 0;
}
.custom-dates input {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
  border: 1px solid var(--border-default);
  border-radius: 10px;
  background: var(--bg-elevated);
  color: inherit;
  padding: 10px 12px;
  font: inherit;
}
.date-help,
.date-error {
  flex-basis: 100%;
  margin: 0;
}
.date-error {
  color: var(--coral);
}
</style>
