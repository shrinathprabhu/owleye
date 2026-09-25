<script setup lang="ts">
import {
  PERFORMANCE_METRICS,
  type PerformanceMetric,
  type PerformanceResponse,
} from "~/types/insights";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

const props = defineProps<{ siteId?: string }>();
const { api } = useApi();
const response = shallowRef<PerformanceResponse | null>(null);
const selectedMetric = ref<PerformanceMetric>("LCP");
const days = ref(30);
const loading = ref(false);
const error = ref("");
let controller: AbortController | undefined;

watch(
  () => props.siteId,
  () => void load(),
  { immediate: true },
);
onBeforeUnmount(() => controller?.abort());

async function load() {
  controller?.abort();
  controller = undefined;
  response.value = null;
  error.value = "";
  loading.value = false;
  const siteId = props.siteId;
  if (!siteId) return;
  const current = new AbortController();
  controller = current;
  loading.value = true;
  try {
    const result = await api<PerformanceResponse>(
      routes.sites.performance(siteId),
      {
        query: { days: days.value, metric: selectedMetric.value },
        signal: current.signal,
      },
    );
    if (!current.signal.aborted && props.siteId === siteId)
      response.value = result;
  } catch (cause) {
    if (!current.signal.aborted) {
      error.value = requestError(cause, "Web Vitals could not be loaded.");
    }
  } finally {
    if (controller === current) {
      controller = undefined;
      loading.value = false;
    }
  }
}

async function selectMetric(metric: PerformanceMetric) {
  if (metric === selectedMetric.value) return;
  selectedMetric.value = metric;
  await load();
}

function summary(metric: PerformanceMetric) {
  return response.value?.metrics.find((item) => item.metric === metric);
}

function value(metric: PerformanceMetric, raw?: number) {
  if (raw === undefined) return "—";
  return metric === "CLS"
    ? raw.toFixed(3)
    : `${Math.round(raw).toLocaleString()} ms`;
}

const selectedSamples = computed(
  () => summary(selectedMetric.value)?.samples ?? 0,
);
const emptyDescription = computed(() =>
  response.value?.collecting
    ? selectedMetric.value === "INP"
      ? "Other metrics are available. INP needs a supported browser and a qualifying interaction; try a wider date range."
      : "Other metrics are available, but none were recorded for this metric in the selected range. Browser support and page lifecycle can affect collection; try a wider range."
    : `No Web Vitals measurements were recorded in the last ${days.value} days. Try a wider range, or check that Web Vitals collection is enabled and receiving visits.`,
);
</script>

<template>
  <div class="insights-stack">
    <div class="insights-toolbar">
      <div>
        <strong>Real-user field data</strong>
        <span>Web Vitals estimates · p75 summaries</span>
      </div>
      <label>
        <span>Range</span>
        <select v-model.number="days" @change="load">
          <option :value="7">Last 7 days</option>
          <option :value="30">Last 30 days</option>
          <option :value="90">Last 90 days</option>
        </select>
      </label>
    </div>

    <section v-if="error" class="insight-panel performance-error" role="alert">
      <div>
        <h2>Web Vitals couldn’t be loaded</h2>
        <p>{{ error }}</p>
        <p>
          Sample availability is unknown because the request failed. Try again.
        </p>
      </div>
      <button type="button" class="button secondary compact" @click="load">
        Retry
      </button>
    </section>

    <section
      v-if="!error"
      class="vitals-grid"
      aria-label="Web Vital summaries"
      :aria-busy="loading"
    >
      <button
        v-for="metric in PERFORMANCE_METRICS"
        :key="metric"
        type="button"
        :class="[
          'vital-card',
          summary(metric)?.rating,
          { active: selectedMetric === metric },
        ]"
        :aria-pressed="selectedMetric === metric"
        @click="selectMetric(metric)"
      >
        <span>{{ metric }}</span>
        <strong>{{ value(metric, summary(metric)?.p75) }}</strong>
        <small>
          {{
            loading
              ? "Loading…"
              : summary(metric)
                ? `${summary(metric)?.samples.toLocaleString()} ${summary(metric)?.samples === 1 ? "sample" : "samples"} · ${summary(metric)!.samples < 3 ? "limited samples" : summary(metric)?.rating.replace("_", " ")}`
                : "No samples"
          }}
        </small>
      </button>
    </section>

    <section v-if="!error" class="insight-panel performance-stage">
      <header>
        <div>
          <p class="panel-kicker">Percentile history</p>
          <h2>{{ selectedMetric }} over time</h2>
        </div>
        <span>p50 · p75 · p95</span>
      </header>
      <p
        v-if="response && selectedSamples > 0 && selectedSamples < 3"
        class="sample-note"
      >
        Only {{ selectedSamples }} {{ selectedMetric }}
        {{ selectedSamples === 1 ? "sample is" : "samples are" }} available.
        Percentiles are preliminary; page rankings need at least 3 samples per
        page.
      </p>
      <PerformanceChart
        :loading="loading"
        :metric="selectedMetric"
        :points="response?.series ?? []"
        :empty-description="emptyDescription"
      />
    </section>

    <section v-if="response?.pages.length" class="insight-panel">
      <header>
        <div>
          <p class="panel-kicker">Needs attention</p>
          <h2>Slowest pages by p75 {{ selectedMetric }}</h2>
        </div>
        <span>Minimum 3 samples</span>
      </header>
      <div class="insight-table-wrap">
        <table class="insight-table">
          <thead>
            <tr>
              <th>Page</th>
              <th>p75</th>
              <th>Samples</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="page in response.pages" :key="page.path">
              <th>{{ page.path }}</th>
              <td>{{ value(selectedMetric, page.p75) }}</td>
              <td>{{ page.samples.toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <section v-if="response && !response.collecting" class="enable-card">
      <div>
        <p class="panel-kicker">Explicit opt-in</p>
        <h2>Check Web Vitals collection</h2>
        <p>
          Pageviews alone do not include Web Vitals. If collection is not
          already enabled, initialize the Web Vitals collector below. An empty
          date range does not necessarily mean collection is disabled. Privacy
          preferences and browser support can also limit measurements.
        </p>
      </div>
      <pre><code>{{ `import { trackWebVitals } from "@owleye/analytics/performance";

const vitals = trackWebVitals(${JSON.stringify(response.site_id)});
// During teardown or when permission is withdrawn:
vitals.stop();` }}</code></pre>
    </section>
  </div>
</template>

<style scoped>
.insights-stack {
  display: grid;
  gap: 18px;
}
.performance-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 16px;
  border-color: var(--error-700);
}
.performance-error p,
.sample-note {
  margin: 8px 0 0;
  color: var(--text-secondary);
  font-size: 0.85rem;
  line-height: 1.5;
}
.insights-toolbar,
.insight-panel > header,
.enable-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
}
.insights-toolbar {
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  background: var(--bg-elevated);
  padding: 13px 16px;
}
.insights-toolbar > div {
  display: grid;
  gap: 2px;
}
.insights-toolbar strong,
h2 {
  color: var(--text-primary);
}
.insights-toolbar span {
  color: var(--text-tertiary);
  font-size: 0.76rem;
}
.insights-toolbar label {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 0.76rem;
  font-weight: 700;
}
select {
  min-height: 38px;
  border: 1px solid var(--border-default);
  border-radius: 9px;
  background: var(--bg-elevated);
  padding: 0 10px;
}
.vitals-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 10px;
}
.vital-card {
  display: grid;
  min-width: 0;
  gap: 4px;
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  color: var(--text-secondary);
  background: var(--bg-elevated);
  padding: 15px;
  text-align: left;
  cursor: pointer;
}
.vital-card.active {
  border-color: var(--text-primary);
  box-shadow: 3px 3px 0 var(--text-primary);
  transform: translate(-1px, -1px);
}
.vital-card > span {
  font-family: var(--font-mono);
  font-size: 0.68rem;
  font-weight: 800;
}
.vital-card strong {
  overflow: hidden;
  color: var(--text-primary);
  font-size: clamp(1.1rem, 2.1vw, 1.55rem);
  text-overflow: ellipsis;
}
.vital-card small {
  color: var(--text-tertiary);
  font-size: 0.68rem;
  text-transform: capitalize;
}
.vital-card.good > span {
  color: var(--success-700);
}
.vital-card.needs_improvement > span {
  color: var(--warning-700);
}
.vital-card.poor > span {
  color: var(--error-700);
}
.insight-panel {
  min-width: 0;
  border: 1px solid var(--border-subtle);
  border-radius: 18px;
  background: var(--bg-elevated);
  padding: clamp(15px, 2.5vw, 23px);
  box-shadow: var(--shadow-xs);
}
.insight-panel > header {
  margin-bottom: 8px;
}
.insight-panel h2,
.enable-card h2 {
  margin: 0;
  font-size: 1.05rem;
}
.insight-panel header > span {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 0.68rem;
}
.insight-table-wrap {
  overflow-x: auto;
}
.insight-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.82rem;
  font-variant-numeric: tabular-nums;
}
.insight-table th,
.insight-table td {
  border-top: 1px solid var(--border-subtle);
  padding: 11px;
  text-align: right;
}
.insight-table th:first-child {
  text-align: left;
}
.insight-table thead th {
  border-top: 0;
  color: var(--text-tertiary);
  font-size: 0.68rem;
  text-transform: uppercase;
}
.enable-card {
  align-items: flex-end;
  border: 1.5px solid var(--text-primary);
  border-radius: 16px;
  background: var(--bg-accent-subtle);
  padding: 18px;
}
.enable-card p:not(.panel-kicker) {
  max-width: 680px;
  margin: 6px 0 0;
  font-size: 0.8rem;
  line-height: 1.5;
}
.enable-card pre {
  margin: 0;
  min-width: 0;
  overflow-x: auto;
}
.enable-card code {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-radius: 8px;
  color: #f7f7f2;
  background: #111214;
  padding: 11px 13px;
  font-size: 0.72rem;
  white-space: pre;
}
@media (max-width: 920px) {
  .vitals-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 600px) {
  .insights-toolbar,
  .enable-card {
    align-items: stretch;
    flex-direction: column;
  }
  .vitals-grid {
    grid-template-columns: 1fr;
  }
}
</style>
