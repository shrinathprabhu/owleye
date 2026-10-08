<script setup lang="ts">
import {
  publicMetricLabels,
  publicBreakdownLabels,
  type PublicOverview,
  type PublicBreakdown,
} from "~/types/publicDashboard";
import { orderedTrafficMetrics } from "~/utils/trafficMetrics";
import {
  breakdownItems,
  breakdownLabel,
  type BreakdownMetric,
} from "~/utils/breakdownMetric";
import { apiErrorStatus } from "~/utils/apiError";
definePageMeta({ alias: ["/p"] });

useHead({
  title: "Shared Overview · OwlEye",
  meta: [
    { name: "robots", content: "noindex, nofollow" },
    { name: "referrer", content: "no-referrer" },
  ],
});
const route = useRoute();
const { publicApi } = useApi();
const report = ref<PublicOverview | null>(null);
const breakdownMetric = ref<BreakdownMetric>("pageviews");
const metricLabel = computed(() => breakdownLabel(breakdownMetric.value));
const audienceTotal = computed(
  () => report.value?.data.audience_totals?.[breakdownMetric.value],
);
const pending = ref(true);
const error = ref("");
const selectedDays = ref<number>();
let controller: AbortController | undefined;
const metrics = computed(() =>
  orderedTrafficMetrics(report.value?.metrics ?? []),
);
const primaryMetrics = computed(() =>
  metrics.value.filter((metric) => metric !== "sessions"),
);
const emptyBreakdownMessage = "No page-view traffic in this range.";
const audienceBreakdowns = computed(() =>
  (["browsers", "operating_systems", "devices"] as PublicBreakdown[])
    .filter((key) => report.value?.data[key] !== undefined)
    .map((key) => ({
      key,
      label: publicBreakdownLabels[key],
      rows: report.value!.data[key]!,
    })),
);
const days = computed(() =>
  [7, 30, 90].filter((day) => day <= (report.value?.max_days ?? 7)),
);
const number = (value?: number) =>
  value === undefined ? "—" : new Intl.NumberFormat().format(value);
onMounted(() => load());
watch(
  () => [route.query.site_id, route.query.site],
  () => {
    selectedDays.value = undefined;
    breakdownMetric.value = "pageviews";
    void load();
  },
);
onBeforeUnmount(() => controller?.abort());
async function load() {
  controller?.abort();
  const request = new AbortController();
  controller = request;
  pending.value = true;
  error.value = "";
  report.value = null;
  const siteId =
    typeof route.query.site_id === "string" ? route.query.site_id : undefined;
  const site =
    typeof route.query.site === "string" ? route.query.site : undefined;
  if ((!siteId && !site) || (siteId && site)) {
    error.value =
      "This link is invalid. Ask the app owner for a published Overview link.";
    pending.value = false;
    return;
  }
  try {
    const response = await publicApi<PublicOverview>("/v1/public/overview", {
      query: { site_id: siteId, site, days: selectedDays.value },
      signal: request.signal,
      retry: 0,
    });
    if (request.signal.aborted) return;
    report.value = response;
    selectedDays.value = response.data.days;
  } catch (cause) {
    if (!request.signal.aborted) {
      const status = apiErrorStatus(cause);
      error.value =
        status === 403 || status === 404
          ? "This dashboard is not publicly available. Its owner may have turned off sharing."
          : status === 429
            ? "This dashboard is busy. Please try again in a minute."
            : "We couldn’t load these statistics. Please try again.";
    }
  } finally {
    if (!request.signal.aborted) pending.value = false;
  }
}
</script>
<template>
  <main id="main-content" class="public-overview">
    <header class="public-brand">
      <a href="https://owleye.dev/" aria-label="OwlEye website">
        <img src="/brand-mark.svg" width="40" height="40" alt="" />
        <strong>OwlEye</strong>
      </a>
      <span>Public Overview</span>
    </header>
    <section v-if="pending" class="public-state" role="status">
      <span class="state-orbit" aria-hidden="true"></span>
      <h1>Loading shared statistics…</h1>
    </section>
    <section v-else-if="error" class="public-state">
      <h1>Dashboard unavailable</h1>
      <p role="alert">{{ error }}</p>
      <button type="button" class="button secondary" @click="load">
        Try again
      </button>
    </section>
    <template v-else-if="report">
      <header class="page-header public-title">
        <div class="page-title">
          <p class="eyebrow">Shared analytics</p>
          <h1>{{ report.name }}</h1>
          <p>{{ report.data.start_date }} – {{ report.data.end_date }} · UTC</p>
        </div>
      </header>
      <section
        class="panel public-range"
        aria-label="Shared analytics date range"
      >
        <label for="public-days"
          >Range
          <select id="public-days" v-model.number="selectedDays" @change="load">
            <option v-for="day in days" :key="day" :value="day">
              {{ day }} days · Daily detail
            </option>
          </select>
        </label>
        <span>Only statistics shared by the app owner</span>
      </section>
      <section
        v-if="primaryMetrics.length"
        class="status-strip public-metrics"
        aria-label="Shared totals"
      >
        <article
          v-for="metric in primaryMetrics"
          :key="metric"
          :class="`metric-${metric}`"
        >
          <span>{{
            metric === "events" ? "Tracked events" : publicMetricLabels[metric]
          }}</span>
          <strong>{{ number(report.data.totals[metric]) }}</strong>
          <small>{{
            metric === "visitors"
              ? "Estimated, deduplicated"
              : `${report.data.start_date} – ${report.data.end_date} (UTC)`
          }}</small>
        </article>
      </section>
      <section class="content-grid" aria-label="Shared analytics charts">
        <article
          v-if="report.data.traffic"
          id="traffic"
          class="panel wide data-stage"
        >
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Trend</p>
              <h2>Traffic</h2>
            </div>
            <span
              >{{ report.data.start_date }} – {{ report.data.end_date }} · Daily
              (UTC)</span
            >
          </div>
          <PublicTrafficChart
            :points="report.data.traffic"
            :metrics="metrics"
            :range-key="`${report.data.start_date}:${report.data.end_date}`"
          />
        </article>
        <article
          v-if="report.data.countries !== undefined"
          class="panel wide geo-panel"
        >
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Geography</p>
              <h2>Countries</h2>
            </div>
            <BreakdownMetricToggle
              v-if="report.data.audience_totals"
              v-model="breakdownMetric"
              label="Geography metric"
            />
            <span v-else>Page views</span>
          </div>
          <CountryGeoChart
            :countries="breakdownItems(report.data.countries, breakdownMetric)"
            :metric-label="metricLabel"
            :empty-message="emptyBreakdownMessage"
          />
        </article>
        <article
          v-if="audienceBreakdowns.length"
          id="audience"
          class="panel wide dimension-panel"
        >
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Audience &amp; technology</p>
              <h2>How people showed up</h2>
            </div>
            <BreakdownMetricToggle
              v-if="report.data.audience_totals"
              v-model="breakdownMetric"
              label="Audience metric"
            />
            <span v-else>Page views</span>
          </div>
          <p class="breakdown-note">
            {{
              breakdownMetric === "visitors"
                ? "Visitors with a page view, deduplicated across this range. A visitor can appear in multiple categories; the center total counts them once."
                : "Page views only. Custom, rule, duration, and performance events are excluded."
            }}
            The metric selection also applies to geography.
          </p>
          <div
            class="dimension-chart-grid"
            :style="{ '--public-dimensions': audienceBreakdowns.length }"
          >
            <section
              v-for="section in audienceBreakdowns"
              :key="section.key"
              :aria-labelledby="`${section.key}-title`"
            >
              <h3 :id="`${section.key}-title`">{{ section.label }}</h3>
              <DimensionDonutChart
                :items="breakdownItems(section.rows, breakdownMetric)"
                :metric-label="metricLabel"
                :total="audienceTotal"
                :label="section.label"
                :empty-message="emptyBreakdownMessage"
              />
            </section>
          </div>
        </article>
        <article v-if="report.data.traffic" class="panel wide public-details">
          <details>
            <summary>Daily data</summary>
            <div class="public-table">
              <table>
                <caption class="sr-only">
                  Daily values for the shared metrics
                </caption>
                <thead>
                  <tr>
                    <th scope="col">Date (UTC)</th>
                    <th v-for="metric in metrics" :key="metric" scope="col">
                      {{ publicMetricLabels[metric] }}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="point in report.data.traffic" :key="point.date">
                    <th scope="row">{{ point.date }}</th>
                    <td v-for="metric in metrics" :key="metric">
                      {{ number(point[metric]) }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </details>
        </article>
      </section>
      <footer>
        <p>
          Only statistics selected by the app owner are shown. Visitor and
          session counts are estimates. Breakdowns use the same counts and
          grouping as Console.
        </p>
        <a href="https://owleye.dev/">Cookie-free analytics by OwlEye ↗</a>
      </footer>
    </template>
  </main>
</template>
<style scoped>
.public-overview {
  width: 100%;
  max-width: var(--content-wide);
  margin-inline: auto;
  padding: 28px var(--layout-gutter) 48px;
  min-height: 100svh;
}
.public-brand {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 24px;
  border-bottom: 1px solid var(--border-subtle);
}
.public-brand a {
  display: flex;
  align-items: center;
  gap: 10px;
  color: inherit;
  text-decoration: none;
}
.public-brand > span {
  font-size: 0.75rem;
  border: 1px solid var(--border-subtle);
  border-radius: 99px;
  padding: 6px 10px;
}
.public-title {
  margin-block: 28px 24px;
}
.public-title h1 {
  overflow-wrap: anywhere;
}
.public-range {
  display: flex;
  align-items: end;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 16px;
  margin-bottom: var(--space-4);
}
.public-range label {
  display: grid;
  gap: 7px;
  min-width: min(100%, 240px);
  font-size: 0.8rem;
  font-weight: 700;
}
.public-range select {
  min-height: 48px;
  padding: 10px 32px 10px 12px;
  border: 1.5px solid var(--border-strong);
  border-radius: var(--radius-lg);
  color: var(--text-primary);
  background: var(--bg-elevated);
  font: inherit;
}
.public-range select:focus-visible {
  outline: 2px solid var(--brand);
  outline-offset: 3px;
}
.public-range > span {
  color: var(--text-tertiary);
  font-size: 0.8rem;
}
.public-metrics {
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 200px), 1fr));
}
.public-metrics .metric-pageviews::before {
  background: var(--brand);
}
.public-metrics .metric-visitors::before {
  background: var(--accent);
}
.public-metrics .metric-events::before {
  background: var(--coral);
}
.dimension-chart-grid {
  grid-template-columns: repeat(var(--public-dimensions), minmax(0, 1fr));
}
.public-details {
  min-height: 0;
}
.public-details summary {
  cursor: pointer;
  font-weight: 650;
}
.public-table {
  overflow: auto;
  max-height: 320px;
  margin-top: 12px;
}
.public-table table {
  width: 100%;
  border-collapse: collapse;
  white-space: nowrap;
}
.public-table th,
.public-table td {
  text-align: left;
  padding: 10px;
  border-bottom: 1px solid var(--border-subtle);
  font-size: 0.85rem;
}
.public-overview footer {
  font-size: 0.8rem;
  line-height: 1.6;
  color: var(--text-secondary);
  margin-top: 32px;
}
.public-state {
  padding: 80px 0;
  max-width: 600px;
}
.public-state h1 {
  font-size: 2rem;
}
.public-state .state-orbit {
  display: block;
  margin-bottom: 24px;
}
@media (max-width: 760px) {
  .dimension-chart-grid {
    grid-template-columns: 1fr;
  }
  .panel-header {
    flex-wrap: wrap;
    gap: 10px;
  }
  .panel-header > span {
    white-space: normal;
    overflow-wrap: anywhere;
  }
}
@media (max-width: 420px) {
  .public-range label {
    width: 100%;
  }
  .public-brand {
    gap: 8px;
  }
}
</style>
