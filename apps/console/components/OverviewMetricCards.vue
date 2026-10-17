<script setup lang="ts">
import { toggleChartMetric } from "~/utils/chartSelection";

type OverviewMetric = "pageviews" | "visitors" | "events";
const props = defineProps<{
  modelValue: OverviewMetric[];
  totals?: { pageviews: number; pageview_visitors: number; events: number };
  caption: string;
  loading?: boolean;
  error?: string;
}>();
const emit = defineEmits<{ "update:modelValue": [value: OverviewMetric[]] }>();
const metricKeys: OverviewMetric[] = ["pageviews", "visitors", "events"];
function toggle(key: string) {
  emit(
    "update:modelValue",
    toggleChartMetric(props.modelValue, key as OverviewMetric, metricKeys),
  );
}
const cards = computed(() => [
  {
    key: "pageviews" as const,
    label: "Page views",
    color: "var(--traffic-pageviews, var(--owleye-lime, #d8ff52))",
    total: props.totals?.pageviews,
    note: "Recorded page views",
  },
  {
    key: "visitors" as const,
    label: "Estimated visitors",
    color: "var(--traffic-visitors, var(--owleye-violet, #a391ff))",
    total: props.totals?.pageview_visitors,
    note: "Pageview audience · deduplicated across the range",
  },
  {
    key: "events" as const,
    label: "Tracked events",
    color: "var(--traffic-events, var(--owleye-coral, #ff705c))",
    total: props.totals?.events,
    note: "All recorded event types",
  },
]);
</script>

<template>
  <section
    class="overview-metrics"
    aria-label="Overview metrics; select a card to view only that series"
    :aria-busy="loading"
  >
    <button
      v-for="card in cards"
      :key="card.key"
      type="button"
      class="overview-metric console-selectable-card"
      :class="{ 'is-selected': modelValue.includes(card.key) }"
      :aria-pressed="modelValue.includes(card.key)"
      aria-controls="traffic"
      @click="emit('update:modelValue', [card.key])"
    >
      <span class="metric-label"
        >{{ card.label }} <span aria-hidden="true">↗</span></span
      >
      <strong :class="{ skeleton: loading && !totals }">{{
        card.total === undefined ? "—" : card.total.toLocaleString()
      }}</strong>
      <small>{{ card.note }}</small>
      <small class="metric-range">{{
        error && totals ? "Last loaded data · refresh failed" : caption
      }}</small>
    </button>
  </section>
  <ChartSeriesControls
    :options="cards"
    :selected="modelValue"
    chart-id="traffic"
    @toggle="toggle"
    @all="emit('update:modelValue', [...metricKeys])"
  />
</template>

<style scoped>
.overview-metrics {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin-bottom: 24px;
}
.overview-metric {
  display: grid;
  gap: 8px;
  min-width: 0;
  padding: 20px;
  text-align: left;
  color: var(--text-primary);
  background: var(--bg-elevated);
  border: 1px solid var(--border-default);
  border-radius: 18px;
  font: inherit;
  cursor: pointer;
}
.overview-metric:focus-visible {
  outline: 3px solid var(--accent-primary, #4057ff);
  outline-offset: 4px;
}
.metric-label {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-weight: 650;
}
.metric-label > span {
  color: var(--accent-primary, #4057ff);
}
.overview-metric strong {
  font-size: clamp(1.8rem, 3vw, 2.6rem);
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
}
.overview-metric small {
  color: var(--text-secondary);
  font-size: 0.75rem;
}
.metric-range {
  color: var(--text-tertiary);
}
@media (max-width: 700px) {
  .overview-metrics {
    grid-template-columns: 1fr;
    gap: 12px;
  }
  .overview-metric {
    padding: 16px;
  }
}
</style>
