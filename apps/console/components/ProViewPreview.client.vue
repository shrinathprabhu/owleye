<script setup lang="ts">
import { downloadCsv, type CsvCell } from "~/utils/csv";
import { BarChart, LineChart, PieChart, ScatterChart } from "echarts/charts";
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
  widgetMetricLabel,
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

function downloadTable() {
  const definition = props.definition;
  const preview = props.preview;
  if (!definition || !preview) return;
  const rows: CsvCell[][] = [["Title", "Period", "Metric", "Group", "Value"]];
  for (const period of [preview.current, preview.previous]) {
    if (!period) continue;
    for (const point of period.points ?? [])
      rows.push([
        definition.title,
        period.label,
        widgetMetricLabel(definition),
        point.period,
        point.value,
      ]);
    for (const step of period.steps ?? [])
      rows.push([
        definition.title,
        period.label,
        definition.funnel?.conversion_window === "same_session"
          ? "Estimated sessions reaching step"
          : "Estimated visitors reaching step",
        step.name,
        step.count,
      ]);
  }
  for (const point of preview.map ?? [])
    rows.push([
      definition.title,
      preview.current?.label,
      widgetMetricLabel(definition),
      point.name,
      point.value,
    ]);
  downloadCsv("owleye-pro-view.csv", rows);
}

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
    if (
      !hasData.value ||
      props.error ||
      isMap.value ||
      props.definition?.kind === "funnel"
    ) {
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
    isMap.value ||
    props.definition.kind === "funnel"
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

  if (
    (definition.visualization === "pie" ||
      definition.visualization === "donut") &&
    !["visitors", "sessions"].includes(definition.metric ?? "events")
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
  const seriesType = ["pie", "donut"].includes(definition.visualization)
    ? "bar"
    : definition.visualization;
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
      <button
        v-if="hasData && !loading && !error"
        class="button compact secondary"
        type="button"
        @click="downloadTable"
      >
        Download CSV
      </button>
      <span v-if="definition" class="preview-chip">
        {{ definition.kind === "funnel" ? "funnel" : definition.visualization }}
      </span>
    </header>

    <p v-if="definition?.kind === 'chart'" class="field-note">
      {{ widgetMetricLabel(definition) }} for the selected event or rule.
      <template v-if="definition.metric && definition.metric !== 'events'"
        >Counts are estimates; the same visitor or session can appear in
        multiple groups.</template
      >
    </p>
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
        :metric-label="widgetMetricLabel(definition)"
        error=""
        hydrate-on-visible
        :loading="false"
      />
      <template v-if="preview?.previous">
        <p>{{ preview.previous.label }}</p>
        <LazyCountryGeoChart
          :countries="previousMapCountries"
          :metric-label="widgetMetricLabel(definition)"
          error=""
          hydrate-on-visible
          :loading="false"
        />
      </template>
    </div>

    <div
      v-else-if="hasData && definition?.kind === 'funnel'"
      class="preview-result"
    >
      <p class="funnel-definition-note">
        Counts are estimates using the first qualifying entry in the period.
        Later attempts do not restart this funnel. Missing or withheld steps are
        not evidence of zero activity.
      </p>
      <FunnelResults
        v-for="(period, index) in [preview?.current, preview?.previous].filter(
          Boolean,
        )"
        :key="index"
        :label="period!.label"
        :steps="period!.steps ?? []"
        :same-session="definition.funnel?.conversion_window === 'same_session'"
      />
    </div>
    <div v-else-if="hasData" class="preview-result">
      <div ref="chartEl" class="preview-chart"></div>
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
            · top 20 groups + Other</template
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

.funnel-definition-note {
  font-size: 0.78rem;
  line-height: 1.6;
  color: #b6b7bb;
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
