<script setup lang="ts">
import { downloadCsv } from "~/utils/csv";
import type { AiPromptResponse, AiMetric, AiChart } from "~/types/ai";
import type { ChartSnapshot } from "~/utils/aiReportExport";
import {
  defaultChart,
  chartOptions,
  chartLabels,
  metricLabels,
  reportLabel,
  reportNotes,
  reportScope,
} from "~/utils/aiReport";
const props = defineProps<{ response: AiPromptResponse; question: string }>();
const evidence = computed(() => props.response.evidence);
const kind = ref<AiChart>(defaultChart(evidence.value));
const metric = ref<AiMetric>(evidence.value.metric ?? "visitors");
const chartRef = ref<{ snapshot: () => ChartSnapshot | undefined } | null>(
  null,
);
const options = computed(() => chartOptions(evidence.value, metric.value));
const visibleKind = computed(() =>
  options.value.includes(kind.value)
    ? kind.value
    : (options.value[0] ?? "none"),
);
const exporting = ref(false);
function downloadTable() {
  const e = evidence.value;
  downloadCsv(`owleye-${e.report}-${e.start_date}-${e.end_date}.csv`, [
    [
      "Start date (UTC)",
      "End date (UTC)",
      "Scope",
      "Group",
      "Events",
      "Pageviews",
      "Visitors",
      "Sessions",
      "Measurement",
    ],
    ...e.rows.map((row) => [
      e.start_date,
      e.end_date,
      reportScope(e),
      row.label,
      row.events,
      row.pageviews,
      row.visitors,
      row.sessions,
      row.value,
    ]),
  ]);
}
function downloadData() {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(evidence.value, null, 2)], {
      type: "application/json",
    }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = "owleye-report.json";
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
const exportError = ref("");
const availableMetrics = computed(() =>
  Object.fromEntries(
    Object.entries(metricLabels).filter(([key]) =>
      evidence.value.metric === "value"
        ? key === "value" || key === "events"
        : key !== "value",
    ),
  ),
);
const hasData = computed(() =>
  evidence.value.rows.some((row) =>
    metric.value === "value"
      ? row.value !== undefined && Number.isFinite(row.value) && row.events > 0
      : (row[metric.value] ?? 0) > 0,
  ),
);
async function download() {
  if (exporting.value) return;
  exporting.value = true;
  exportError.value = "";
  try {
    const snapshot =
      visibleKind.value !== "none" && hasData.value
        ? chartRef.value?.snapshot()
        : undefined;
    const { exportAiReport } = await import("~/utils/aiReportExport");
    await exportAiReport(
      props.question,
      props.response,
      metric.value,
      snapshot,
    );
  } catch {
    exportError.value = "The report could not be downloaded. Please try again.";
  } finally {
    exporting.value = false;
  }
}
</script>
<template>
  <div class="ai-report">
    <p
      v-if="response.explanation_source === 'fallback'"
      class="ai-report-note"
      role="status"
    >
      Evidence summary · the AI explanation was unavailable or did not pass
      validation.
    </p>
    <p class="ai-answer">{{ response.answer }}</p>
    <div v-if="options.length" class="ai-report-controls">
      <label
        >Chart
        <select v-model="kind" aria-label="Chart type">
          <option v-for="option in options" :key="option" :value="option">
            {{ chartLabels[option] }}
          </option>
        </select>
      </label>
      <label
        >Metric
        <select v-model="metric" aria-label="Chart metric">
          <option
            v-for="(label, value) in availableMetrics"
            :key="value"
            :value="value"
          >
            {{ label }}
          </option>
        </select>
      </label>
    </div>
    <div v-if="visibleKind !== 'none'" class="ai-chart-panel">
      <h3>{{ metricLabels[metric] }} · {{ evidence.report }}</h3>
      <AiReportChart
        v-if="hasData"
        ref="chartRef"
        :evidence="evidence"
        :kind="visibleKind"
        :metric="metric"
      />
      <p v-else>
        No reportable {{ metricLabels[metric].toLowerCase() }} for this chart.
      </p>
      <p v-if="['pie', 'donut'].includes(visibleKind)">
        Shares are relative to displayed buckets. Visitors and sessions can
        appear in more than one bucket.
      </p>
    </div>
    <div v-if="evidence.output === 'pdf'" class="ai-report-downloads">
      <button
        class="button compact secondary"
        type="button"
        :disabled="exporting"
        @click="download"
      >
        {{ exporting ? "Preparing…" : "Download PDF report" }}
      </button>
    </div>
    <p v-if="exportError" role="alert">{{ exportError }}</p>
    <button
      type="button"
      class="text-button"
      :disabled="!evidence.rows.length"
      @click="downloadTable"
    >
      Download CSV
    </button>
    <button type="button" class="text-button" @click="downloadData">
      Download supporting data (JSON)
    </button>
    <details>
      <summary>
        Supporting data · {{ evidence.rows.length }}
        {{ evidence.rows.length === 1 ? "row" : "rows" }}
      </summary>
      <p class="ai-report-scope">
        {{ evidence.start_date }} – {{ evidence.end_date }} UTC ·
        {{ reportScope(evidence) }}
      </p>
      <button
        v-if="evidence.output !== 'pdf'"
        class="ai-pdf-link"
        type="button"
        :disabled="exporting"
        @click="download"
      >
        {{ exporting ? "Preparing…" : "Download PDF" }}
      </button>
      <p
        v-for="note in reportNotes(evidence)"
        :key="note"
        class="ai-report-note"
      >
        {{ note }}
      </p>
      <p v-if="!evidence.rows.length">No matching report rows.</p>
      <div
        v-else
        class="ai-evidence-table"
        tabindex="0"
        role="region"
        aria-label="Supporting analytics data"
      >
        <table>
          <thead>
            <tr>
              <th scope="col">Group</th>
              <th v-if="evidence.report !== 'funnel'" scope="col">Events</th>
              <th v-if="evidence.report !== 'funnel'" scope="col">Pageviews</th>
              <th v-if="evidence.metric === 'value'" scope="col">
                Measurement
              </th>
              <th scope="col">Visitors</th>
              <th v-if="evidence.report !== 'funnel'" scope="col">Sessions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in evidence.rows" :key="row.label">
              <th scope="row">{{ reportLabel(row.label, evidence.report) }}</th>
              <td v-if="evidence.report !== 'funnel'">
                {{ row.events.toLocaleString() }}
              </td>
              <td v-if="evidence.report !== 'funnel'">
                {{ row.pageviews.toLocaleString() }}
              </td>
              <td v-if="evidence.metric === 'value'">
                {{ row.value?.toLocaleString() ?? "—" }}
              </td>
              <td>{{ row.visitors.toLocaleString() }}</td>
              <td v-if="evidence.report !== 'funnel'">
                {{ row.sessions.toLocaleString() }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </details>
  </div>
</template>
<style scoped>
.ai-report {
  min-width: 0;
  width: 100%;
}
.ai-answer {
  white-space: pre-wrap;
  margin: 0 0 14px;
  color: #eeeff7;
  font-size: 15px;
  line-height: 1.85;
  letter-spacing: -0.01em;
}
.ai-report-scope,
.ai-report-note {
  font-size: 0.8rem;
  color: #bfc1ca;
  line-height: 1.6;
}
.ai-report-controls,
.ai-report-downloads {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  margin: 16px 0;
}
.ai-report-controls label {
  display: grid;
  gap: 6px;
  font-size: 0.8rem;
  flex: 1 1 120px;
  max-width: 220px;
}
.ai-report-controls select {
  width: 100%;
  color: #fffef9;
  background: #202228;
  border: 1px solid #555862;
  border-radius: 8px;
  padding: 8px;
}
.ai-chart-panel {
  border: 1px solid #34363d;
  border-radius: 12px;
  padding: 12px;
  overflow: hidden;
}
.ai-chart-panel h3 {
  font-size: 1rem;
  margin: 0 0 8px;
}
.ai-chart-panel p {
  font-size: 0.8rem;
  line-height: 1.5;
}
.ai-evidence-table {
  max-width: 100%;
  overflow-x: auto;
  margin-top: 0.75rem;
}
table {
  border-collapse: collapse;
  width: 100%;
  font-variant-numeric: tabular-nums;
}
th,
td {
  text-align: right;
  padding: 0.6rem;
  border-bottom: 1px solid #8884;
  white-space: nowrap;
}
th:first-child {
  text-align: left;
}
.ai-pdf-link {
  color: #bfc1ca;
  background: none;
  border: 0;
  text-decoration: underline;
  cursor: pointer;
  padding: 12px 0;
}
details {
  border-top: 1px solid #ffffff0b;
  margin-top: 16px;
  padding-top: 12px;
}
summary {
  cursor: pointer;
  color: #aaaec6;
  font-size: 11px;
  font-weight: 500;
  width: fit-content;
}
summary:hover {
  color: #dedcff;
}
summary:focus-visible {
  outline: 2px solid #b9b0ff;
  outline-offset: 4px;
}
@media (max-width: 600px) {
  .ai-answer {
    font-size: 14px;
  }
}
</style>

<style scoped>
.text-button {
  color: #dbe7ff;
}
</style>
