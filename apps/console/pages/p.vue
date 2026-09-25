<script setup lang="ts">
import {
  publicMetricLabels,
  publicBreakdownLabels,
  type PublicOverview,
  type PublicBreakdown,
} from "~/types/publicDashboard";
import { countryLabel } from "~/utils/countryGeo";
import { apiErrorStatus } from "~/utils/apiError";
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
const pending = ref(true);
const error = ref("");
const selectedDays = ref<number>();
let controller: AbortController | undefined;
const breakdowns = computed(() =>
  Object.entries(publicBreakdownLabels)
    .filter(([key]) => report.value?.data[key as PublicBreakdown] !== undefined)
    .map(([key, label]) => ({
      key: key as PublicBreakdown,
      label,
      rows: report.value!.data[key as PublicBreakdown]!,
    })),
);
const days = computed(() =>
  [7, 30, 90].filter((day) => day <= (report.value?.max_days ?? 7)),
);
const number = (value?: number) => new Intl.NumberFormat().format(value ?? 0);
onMounted(() => load());
watch(
  () => [route.query.site_id, route.query.site],
  () => {
    selectedDays.value = undefined;
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
  <main class="public-overview">
    <header class="public-brand">
      <a href="https://owleye.dev/" aria-label="OwlEye website"
        ><img src="/brand-mark.svg" width="40" height="40" alt="" /><strong
          >OwlEye</strong
        ></a
      ><span>Public Overview</span>
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
      <header class="public-title">
        <div>
          <p class="eyebrow">A shared view of the numbers</p>
          <h1>{{ report.name }}</h1>
          <p>{{ report.data.start_date }} – {{ report.data.end_date }} · UTC</p>
        </div>
        <label
          >History<select
            aria-label="History"
            v-model.number="selectedDays"
            @change="load"
          >
            <option v-for="day in days" :key="day" :value="day">
              Last {{ day }} days
            </option>
          </select></label
        >
      </header>
      <section class="public-metrics" aria-label="Shared totals">
        <article v-for="metric in report.metrics" :key="metric">
          <p>{{ publicMetricLabels[metric] }}</p>
          <strong>{{ number(report.data.totals[metric]) }}</strong>
        </article>
      </section>
      <section v-if="report.data.traffic" class="public-panel">
        <h2>Traffic over time</h2>
        <ClientOnly
          ><PublicTrafficChart
            :points="report.data.traffic"
            :metrics="report.metrics"
        /></ClientOnly>
        <details>
          <summary>Daily data</summary>
          <div class="public-table">
            <table>
              <thead>
                <tr>
                  <th>Date (UTC)</th>
                  <th v-for="metric in report.metrics" :key="metric">
                    {{ publicMetricLabels[metric] }}
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="point in report.data.traffic" :key="point.date">
                  <td>{{ point.date }}</td>
                  <td v-for="metric in report.metrics" :key="metric">
                    {{ number(point[metric]) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </details>
      </section>
      <div class="public-breakdowns">
        <section
          v-for="section in breakdowns"
          :key="section.key"
          class="public-panel"
        >
          <header>
            <h2>{{ section.label }}</h2>
            <span>Page views</span>
          </header>
          <ol v-if="section.rows.length">
            <li v-for="row in section.rows" :key="row.name">
              <span>{{
                section.key === "countries" ? countryLabel(row.name) : row.name
              }}</span
              ><strong>{{ number(row.count) }}</strong>
              <div
                aria-hidden="true"
                :style="{
                  width: `${Math.max(1, (row.count / Math.max(1, ...section.rows.map((r) => r.count))) * 100)}%`,
                }"
              ></div>
            </li>
          </ol>
          <p v-else>
            No groups meet the minimum of 5 visitors for this period.
          </p>
        </section>
      </div>
      <footer>
        <p>
          Only statistics selected by the app owner are shown. Visitor and
          session counts are estimates. Breakdowns show up to 20 groups with at
          least 5 visitors.
        </p>
        <a href="https://owleye.dev/">Cookie-free analytics by OwlEye ↗</a>
      </footer>
    </template>
  </main>
</template>
<style scoped>
.public-overview {
  max-width: 1180px;
  margin: 0 auto;
  padding: 28px clamp(16px, 4vw, 48px) 48px;
  min-height: 100svh;
}
.public-brand {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 28px;
  border-bottom: 1px solid #d6d2c8;
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
  border: 1px solid #c8c3b7;
  border-radius: 99px;
  padding: 6px 10px;
}
.public-title {
  display: flex;
  align-items: end;
  justify-content: space-between;
  gap: 20px;
  flex-wrap: wrap;
  margin: 36px 0 24px;
}
.public-title h1 {
  font-size: clamp(2rem, 5vw, 3.5rem);
  margin: 8px 0;
  overflow-wrap: anywhere;
}
.public-title p {
  color: var(--text-secondary);
  font-size: 0.9rem;
}
.public-title label {
  display: grid;
  gap: 7px;
  font-size: 0.8rem;
}
.public-title select {
  min-height: 42px;
  padding: 9px 32px 9px 12px;
  border: 1px solid #bdb7a9;
  border-radius: 10px;
  color: var(--text-primary);
  background: #fffefb;
  font: inherit;
}
.public-title select:focus-visible {
  outline: 2px solid #4255ed;
  outline-offset: 2px;
}
.public-metrics {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 14px;
  margin-bottom: 22px;
}
.public-metrics article,
.public-panel {
  background: #fffefb;
  border: 1px solid #d6d2c8;
  border-radius: 16px;
  padding: 22px;
  min-width: 0;
}
.public-metrics p {
  font-size: 0.85rem;
  margin: 0 0 10px;
  color: var(--text-secondary);
}
.public-metrics strong {
  font-size: 2rem;
  letter-spacing: -0.04em;
}
.public-panel h2 {
  font-size: 1.1rem;
  margin: 0 0 16px;
}
.public-panel details {
  margin-top: 18px;
  font-size: 0.8rem;
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
  border-bottom: 1px solid #e9e6de;
}
.public-breakdowns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr));
  gap: 20px;
  margin-top: 22px;
}
.public-panel header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.public-panel header > span {
  font-size: 0.75rem;
  color: var(--text-secondary);
}
.public-panel ol {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 16px;
}
.public-panel li {
  position: relative;
  display: flex;
  justify-content: space-between;
  gap: 10px;
  padding: 10px;
  font-size: 0.85rem;
  isolation: isolate;
  overflow-wrap: anywhere;
}
.public-panel li > div {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  border-radius: 5px;
  background: #eceefd;
  z-index: -1;
}
.public-panel > p {
  font-size: 0.85rem;
  color: var(--text-secondary);
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
@media (max-width: 420px) {
  .public-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .public-metrics article {
    padding: 16px;
  }
  .public-metrics strong {
    font-size: 1.65rem;
  }
  .public-panel {
    padding: 16px;
  }
}
</style>
