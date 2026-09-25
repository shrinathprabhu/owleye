<script setup lang="ts">
import {
  BarChart,
  FunnelChart,
  LineChart,
  PieChart,
  ScatterChart,
} from "echarts/charts";
import {
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { use, type EChartsCoreOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

import { useEChartLifecycle } from "~/composables/useEChartLifecycle";
import {
  widgetRangeLabel,
  breakdownLabel,
  type ProViewPreviewResponse,
  type ProViewWidgetDefinition,
} from "~/types/pro-view";
import { chartMotion, chartZoom, readChartTheme } from "~/utils/chartTheme";

use([
  AriaComponent,
  BarChart,
  CanvasRenderer,
  DataZoomComponent,
  FunnelChart,
  GridComponent,
  LegendComponent,
  LineChart,
  PieChart,
  ScatterChart,
  TooltipComponent,
]);

const props = withDefaults(
  defineProps<{
    definition?: ProViewWidgetDefinition | null;
    error?: string;
    loading?: boolean;
    preview?: ProViewPreviewResponse | null;
  }>(),
  {
    definition: null,
    error: "",
    loading: false,
    preview: null,
  },
);

const emit = defineEmits<{ retry: [] }>();
const chartEl = ref<HTMLElement | null>(null);
const isMap = computed(() => props.definition?.visualization === "map");
const windowLabel = computed(() => {
  const window = props.definition?.funnel?.conversion_window;
  return window
    ? {
        same_session: "within the same session",
        "1h": "within 1 hour",
        "1d": "within 1 day",
        "7d": "within 7 days",
        "30d": "within 30 days",
      }[window]
    : "no window selected";
});
const mapCountries = computed(() =>
  (props.preview?.map ?? []).map((country) => ({
    count: country.value,
    name: country.name,
  })),
);
const previousMapCountries = computed(() =>
  (props.preview?.previous?.points ?? []).map((point) => ({
    name: point.period,
    count: point.value,
  })),
);
const hasData = computed(() => {
  if (isMap.value)
    return (
      mapCountries.value.length > 0 || previousMapCountries.value.length > 0
    );
  if (props.definition?.kind === "funnel") {
    return [
      ...(props.preview?.current?.steps ?? []),
      ...(props.preview?.previous?.steps ?? []),
    ].some((step) => step.count > 0);
  }
  return Boolean(
    props.preview?.current?.points?.length ||
    props.preview?.previous?.points?.length,
  );
});

const { clearChart, ensureChart, prefersReducedMotion } = useEChartLifecycle(
  chartEl,
  renderChart,
  { observeTheme: true },
);

onMounted(async () => {
  await nextTick();
  renderChart();
});

watch(
  () => [props.definition, props.error, props.loading, props.preview] as const,
  async () => {
    await nextTick();
    if (!hasData.value || props.error || isMap.value) {
      clearChart();
      return;
    }
    renderChart();
  },
);

function renderChart() {
  if (
    !chartEl.value ||
    !props.definition ||
    !props.preview ||
    !hasData.value ||
    props.error ||
    isMap.value
  ) {
    return;
  }

  const chart = ensureChart();
  if (!chart) return;
  chart.setOption(buildOption(), {
    lazyUpdate: false,
    notMerge: true,
    silent: false,
  });
}

function buildOption(): EChartsCoreOption {
  const definition = props.definition!;
  const preview = props.preview!;
  const theme = readChartTheme(chartEl.value!);
  const motion = chartMotion(prefersReducedMotion(), 620);
  const shared = {
    ...motion,
    aria: {
      description: `${definition.title}. ${widgetRangeLabel(definition)}. Values are queried by the OwlEye API.`,
      enabled: true,
    },
    color: theme.series,
    textStyle: {
      color: theme.text,
      fontFamily: "Geist Variable, sans-serif",
    },
    tooltip: {
      backgroundColor: theme.tooltipBg,
      borderColor: theme.axis,
      textStyle: { color: theme.tooltipText },
      trigger:
        definition.visualization === "pie" ||
        definition.visualization === "donut" ||
        definition.kind === "funnel"
          ? "item"
          : "axis",
    },
  };

  if (definition.kind === "funnel") {
    const cohorts = [preview.current, preview.previous].filter(
      (cohort) => cohort?.steps?.length,
    );
    return {
      ...shared,
      legend: {
        bottom: 0,
        textStyle: { color: theme.text },
      },
      series: cohorts.map((cohort, index) => ({
        data: cohort!.steps!.map((step) => ({
          ...step,
          value: step.count,
        })),
        gap: 4,
        itemStyle: { opacity: index === 0 ? 1 : 0.58 },
        label: {
          color: theme.tooltipText,
          fontWeight: 700,
          formatter: (params: {
            data?: { name?: string; overall_conversion?: number };
          }) =>
            `${params.data?.name ?? "Step"}\n${formatPercent(params.data?.overall_conversion ?? 0)} overall`,
          position: "inside",
        },
        left: cohorts.length === 1 ? "10%" : index === 0 ? "4%" : "54%",
        maxSize: "92%",
        minSize: "24%",
        name: cohort!.label,
        sort: "none" as const,
        top: 12,
        type: "funnel" as const,
        width: cohorts.length === 1 ? "80%" : "42%",
      })),
    };
  }

  if (
    definition.visualization === "pie" ||
    definition.visualization === "donut"
  ) {
    const isDonut = definition.visualization === "donut";
    const hasPrevious = Boolean(preview.previous?.points?.length);
    const series: EChartsCoreOption[] = [];
    const currentData = (preview.current?.points ?? []).map((point) => ({
      name: point.period,
      value: point.value,
    }));
    series.push({
      data: currentData,
      label: { color: theme.text, formatter: "{b}" },
      name: preview.current?.label ?? "Current",
      radius: hasPrevious
        ? isDonut
          ? ["56%", "76%"]
          : ["50%", "76%"]
        : isDonut
          ? ["48%", "74%"]
          : [0, "74%"],
      type: "pie",
    });
    if (hasPrevious) {
      series.push({
        data: preview.previous!.points!.map((point) => ({
          name: point.period,
          value: point.value,
        })),
        itemStyle: { opacity: 0.54 },
        label: { show: false },
        name: preview.previous!.label,
        radius: ["24%", "44%"],
        type: "pie",
      });
    }
    return {
      ...shared,
      legend: {
        bottom: 0,
        textStyle: { color: theme.text },
      },
      series,
    };
  }

  const periods = Array.from(
    new Set([
      ...(preview.current?.points?.map((point) => point.period) ?? []),
      ...(preview.previous?.points?.map((point) => point.period) ?? []),
    ]),
  );
  const seriesType = definition.visualization;
  const makeSeries = (
    period: ProViewPreviewResponse["current"],
    color: string,
  ) => {
    if (!period?.points) return null;
    const byPeriod = new Map(
      period.points.map((point) => [point.period, point.value] as const),
    );
    if (seriesType === "scatter") {
      return {
        data: periods.map((label, index) => [
          index,
          byPeriod.get(label) ?? null,
        ]),
        itemStyle: { color },
        name: period.label,
        symbolSize: 11,
        type: "scatter" as const,
      };
    }
    return {
      data: periods.map((label) => byPeriod.get(label) ?? null),
      itemStyle: { color },
      lineStyle: {
        cap: "round" as const,
        color,
        join: "round" as const,
        shadowBlur: 12,
        shadowColor: color,
        width: 3,
      },
      name: period.label,
      showSymbol: seriesType === "bar" || periods.length === 1,
      smooth: seriesType === "line" ? 0.34 : false,
      type: seriesType as "bar" | "line",
    };
  };
  return {
    ...shared,
    dataZoom:
      definition.breakdown &&
      definition.breakdown !== "time" &&
      periods.length <= 8
        ? []
        : chartZoom(periods.length, theme),
    grid: { bottom: 112, containLabel: true, left: 8, right: 12, top: 28 },
    legend: { bottom: 0, textStyle: { color: theme.text } },
    series: [
      makeSeries(preview.current, theme.series[0] ?? "#3d5afe"),
      makeSeries(preview.previous, theme.series[1] ?? "#007d84"),
    ].filter(Boolean),
    xAxis: {
      axisLabel: { color: theme.text, hideOverlap: true },
      axisLine: { lineStyle: { color: theme.axis } },
      axisTick: { show: false },
      data: periods,
      type: "category",
    },
    yAxis: {
      axisLabel: { color: theme.text },
      axisLine: { show: false },
      axisTick: { show: false },
      minInterval: 1,
      splitLine: { lineStyle: { color: theme.grid } },
      type: "value",
    },
  };
}

function formatPercent(value: number) {
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value)}%`;
}

function formatDuration(value?: number | null) {
  if (value === null || value === undefined) return "—";
  if (value < 60_000) return `${Math.round(value / 1000)}s`;
  if (value < 3_600_000) return `${Math.round(value / 60_000)}m`;
  if (value < 86_400_000) {
    return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value / 3_600_000)}h`;
  }
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value / 86_400_000)}d`;
}
</script>

<template>
  <section class="preview-stage" aria-labelledby="pro-preview-title">
    <header>
      <div>
        <p>Query preview</p>
        <h2 id="pro-preview-title">
          {{ definition?.title || "Nothing selected yet" }}
        </h2>
      </div>
      <span v-if="definition" class="preview-chip">
        {{ definition.kind === "funnel" ? "funnel" : definition.visualization }}
      </span>
    </header>

    <div v-if="loading" class="preview-state" role="status">
      <span class="preview-orbit" aria-hidden="true"></span>
      <strong>Loading preview…</strong>
      <span>Calculating results for the selected steps and date range.</span>
    </div>

    <div v-else-if="error" class="preview-state error" role="alert">
      <span class="state-mark" aria-hidden="true">!</span>
      <strong>Preview couldn’t be loaded</strong>
      <span>{{ error }}</span>
      <span
        >The request failed; this does not mean there was no matching
        activity.</span
      >
      <button
        v-if="definition"
        type="button"
        class="button secondary compact"
        @click="emit('retry')"
      >
        Retry preview
      </button>
    </div>

    <div v-else-if="isMap && hasData" class="preview-result">
      <p>{{ preview?.current?.label }}</p>
      <LazyCountryGeoChart
        :countries="mapCountries"
        metric-label="Events"
        error=""
        hydrate-on-visible
        :loading="false"
      />
      <template v-if="preview?.previous">
        <p>{{ preview.previous.label }}</p>
        <LazyCountryGeoChart
          :countries="previousMapCountries"
          metric-label="Events"
          error=""
          hydrate-on-visible
          :loading="false"
        />
      </template>
    </div>

    <div v-else-if="hasData" class="preview-result">
      <div ref="chartEl" class="preview-chart"></div>
      <div
        v-if="definition?.kind === 'funnel' && preview?.current?.steps"
        class="funnel-metrics"
      >
        <h3>{{ preview.current.label }} conversion receipt</h3>
        <div class="funnel-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Step</th>
                <th>
                  {{
                    definition.funnel?.conversion_window === "same_session"
                      ? "Sessions"
                      : "Anonymous visitors"
                  }}
                </th>
                <th>From prior</th>
                <th>Overall</th>
                <th>Dropoff</th>
                <th>Dropoff rate</th>
                <th>Avg. to next</th>
                <th>Median to next</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="step in preview.current.steps" :key="step.id">
                <th>{{ step.name }}</th>
                <td>{{ new Intl.NumberFormat().format(step.count) }}</td>
                <td>{{ formatPercent(step.step_conversion) }}</td>
                <td>{{ formatPercent(step.overall_conversion) }}</td>
                <td>
                  {{ new Intl.NumberFormat().format(step.dropoff_count) }}
                </td>
                <td>{{ formatPercent(step.dropoff_rate) }}</td>
                <td>{{ formatDuration(step.avg_time_to_next_ms) }}</td>
                <td>{{ formatDuration(step.median_time_to_next_ms) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <div v-else-if="preview" class="preview-state">
      <span class="state-mark" aria-hidden="true">—</span>
      <strong>{{
        definition?.kind === "funnel"
          ? "No matching funnel entries"
          : "No matching data"
      }}</strong>
      <span
        >The query completed, but no matching activity was found in the selected
        periods. Check
        {{
          definition?.kind === "funnel"
            ? "the first step’s path or event"
            : "the selected source and grouping"
        }}, its filters, and the date range.</span
      >
    </div>

    <div v-else class="preview-state">
      <span class="state-mark" aria-hidden="true">↗</span>
      <strong>
        {{ definition ? "Ready for a real query" : "Build a chart or funnel" }}
      </strong>
      <span v-if="definition">
        {{ widgetRangeLabel(definition) }}. Press preview when the definition is
        ready.
      </span>
      <span v-else>
        Pick an existing event, a tracking rule, or define the name of a future
        event.
      </span>
    </div>

    <footer v-if="definition">
      <span
        >{{ widgetRangeLabel(definition)
        }}<template v-if="definition.kind === 'chart'">
          · {{ breakdownLabel(definition)
          }}<template
            v-if="
              definition.breakdown && definition.breakdown !== 'time' && !isMap
            "
          >
            · up to 20 groups</template
          ></template
        ></span
      >
      <span>
        {{
          definition.kind === "funnel"
            ? `${definition.funnel?.steps.length ?? 0} ordered steps · ${windowLabel}`
            : definition.source?.name || "Source not selected"
        }}
      </span>
    </footer>
  </section>
</template>

<style scoped>
.preview-stage {
  --preview-border: #34363d;
  --chart-text: #c4c5c0;
  --chart-axis: #737780;
  --chart-grid: #34363d;
  --chart-2: #63cbd0;
  min-width: 0;
  border: 2px solid #111214;
  border-radius: 20px;
  color: #c4c5c0;
  background:
    radial-gradient(circle at 92% 8%, rgb(61 90 254 / 0.3), transparent 18rem),
    linear-gradient(rgb(255 255 255 / 0.035) 1px, transparent 1px),
    linear-gradient(90deg, rgb(255 255 255 / 0.035) 1px, transparent 1px),
    #111214;
  background-size:
    auto,
    28px 28px,
    28px 28px,
    auto;
  padding: clamp(16px, 3vw, 24px);
  box-shadow: 6px 6px 0 #3d5afe;
}

header,
footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

header {
  margin-bottom: 16px;
}

header p {
  margin: 0 0 2px;
  color: #d8ff52;
  font-family: var(--font-mono);
  font-size: 0.64rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

h2 {
  margin: 0;
  color: #f7f7f2;
  font-size: 1.08rem;
  letter-spacing: -0.02em;
}

.preview-chip {
  border: 1px solid #555861;
  border-radius: 999px;
  color: #d8ff52;
  background: #18191d;
  padding: 5px 9px;
  font-family: var(--font-mono);
  font-size: 0.64rem;
  font-weight: 760;
  text-transform: uppercase;
}

.preview-chart,
.preview-state {
  min-height: 420px;
}

.preview-chart {
  width: 100%;
}

.preview-result {
  min-width: 0;
}

.funnel-metrics {
  margin-top: 14px;
  border: 1px solid #34363d;
  border-radius: 12px;
  background: rgb(24 25 29 / 0.78);
  padding: 12px;
}

.funnel-metrics h3 {
  margin: 0 0 9px;
  color: #f7f7f2;
  font-size: 0.8rem;
}

.funnel-table-wrap {
  overflow-x: auto;
}

table {
  width: 100%;
  min-width: 510px;
  border-collapse: collapse;
  font-size: 0.72rem;
  font-variant-numeric: tabular-nums;
}

th,
td {
  border-top: 1px solid #34363d;
  padding: 8px;
  text-align: right;
}

thead th {
  border-top: 0;
  color: #a8a9ad;
  font-family: var(--font-mono);
  font-size: 0.6rem;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

th:first-child,
td:first-child {
  text-align: left;
}

tbody th {
  color: #f7f7f2;
  font-weight: 680;
}

.preview-state {
  display: grid;
  place-items: center;
  align-content: center;
  gap: 7px;
  border: 1px dashed #555861;
  border-radius: 12px;
  background: rgb(24 25 29 / 0.68);
  padding: 24px;
  text-align: center;
}

.preview-state strong {
  color: #f7f7f2;
  font-size: 0.95rem;
}

.preview-state > span:last-child {
  max-width: 430px;
  color: #a8a9ad;
  font-size: 0.8rem;
  line-height: 1.5;
}

.preview-state.error {
  border-color: #ff8270;
}

.state-mark,
.preview-orbit {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  margin-bottom: 3px;
  border-radius: 50%;
}

.state-mark {
  color: #151515;
  background: #d8ff52;
  font-weight: 850;
}

.error .state-mark {
  background: #ff8270;
}

.preview-orbit {
  border: 2px solid #555861;
  border-top-color: #d8ff52;
  animation: preview-orbit 0.72s linear infinite;
}

footer {
  flex-wrap: wrap;
  margin-top: 14px;
  color: #a8a9ad;
  font-family: var(--font-mono);
  font-size: 0.66rem;
}

@keyframes preview-orbit {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 520px) {
  .preview-stage {
    padding: 14px;
    box-shadow: 3px 3px 0 #3d5afe;
  }

  .preview-chart,
  .preview-state {
    min-height: 330px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .preview-orbit {
    animation: none;
  }
}
</style>
