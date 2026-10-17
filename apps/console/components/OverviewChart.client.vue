<script setup lang="ts">
import { LineChart, type LineSeriesOption } from "echarts/charts";
import {
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  MarkPointComponent,
  TooltipComponent,
  type AriaComponentOption,
  type DataZoomComponentOption,
  type GridComponentOption,
  type LegendComponentOption,
  type TooltipComponentOption,
} from "echarts/components";
import { use, type ComposeOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

import { referrerIconSource } from "~/utils/referrerIcons";
import {
  chartText,
  spikePublicUrl,
  incompleteTrafficPeriod,
  trafficLineSegments,
  type TrafficSpike,
  type TrafficSpikeSource,
} from "~/utils/trafficPresentation";
import { chartSnapshot } from "~/utils/chartSnapshot";
import { useEChartLifecycle } from "~/composables/useEChartLifecycle";
import {
  trafficPeriodLabel,
  type AnalyticsInterval,
} from "~/utils/analyticsRange";
import {
  trafficMetricLabels,
  type TrafficChartPoint,
  type TrafficMetric,
} from "~/utils/trafficMetrics";
import {
  chartLineShape,
  chartLineStroke,
  chartLegend,
  chartPlotGrid,
  chartMotion,
  chartZoom,
  readTrafficChartTheme,
} from "~/utils/chartTheme";

type OverviewChartOption = ComposeOption<
  | AriaComponentOption
  | DataZoomComponentOption
  | GridComponentOption
  | LegendComponentOption
  | LineSeriesOption
  | TooltipComponentOption
>;

use([
  AriaComponent,
  CanvasRenderer,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  MarkPointComponent,
  LineChart,
  TooltipComponent,
]);

const props = withDefaults(
  defineProps<{
    interval?: AnalyticsInterval;
    asOf?: string;
    spikes?: TrafficSpike[];
    rangeKey?: string;
    error?: string;
    loading?: boolean;
    points?: TrafficChartPoint[];
    metrics?: TrafficMetric[];
  }>(),
  {
    interval: "day",
    asOf: "",
    spikes: () => [],
    rangeKey: "",
    error: "",
    loading: false,
    points: () => [],
    metrics: () => ["pageviews", "visitors", "events"],
  },
);

const emit = defineEmits<{
  retry: [];
}>();

let renderedRange = "";
let boundChart: ReturnType<typeof ensureChart>;
const chartEl = ref<HTMLElement | null>(null);
const periods = computed(() =>
  props.points
    .map((point) => point.date)
    .sort((left, right) => left.localeCompare(right)),
);
const focusedIndex = ref<number | null>(null);
const visibleSpikes = computed(() =>
  props.interval === "day" && props.metrics.includes("pageviews")
    ? props.spikes
        .filter((spike) => periods.value.includes(spike.date))
        .slice(0, 4)
        .map((spike) => ({
          ...spike,
          sources: spike.sources.slice(0, 3).map((source) => ({
            ...source,
            publicUrl: spikePublicUrl(source.publicUrl),
          })),
        }))
    : [],
);
const hasPartial = computed(() =>
  periods.value.some((date) =>
    incompleteTrafficPeriod(date, props.interval, props.asOf),
  ),
);
const hasPoints = computed(() => periods.value.length > 0);
const trafficByDate = computed(
  () => new Map(props.points.map((point) => [point.date, point] as const)),
);
const chartLabel = computed(
  () =>
    `Traffic for ${periods.value.length} time periods. ${props.metrics.map((metric) => trafficMetricLabels[metric]).join(", ")}.`,
);

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
  () =>
    [
      props.loading,
      props.error,
      props.points,
      props.metrics,
      props.rangeKey,
      props.interval,
      props.asOf,
      props.spikes,
    ] as const,
  async () => {
    await nextTick();
    if (!hasPoints.value || props.error) {
      clearChart();
      return;
    }
    if (props.loading) return;
    renderChart();
  },
);

function renderChart() {
  if (!chartEl.value || !hasPoints.value || props.error) return;
  const currentChart = ensureChart();
  if (!currentChart) return;
  if (boundChart !== currentChart) {
    boundChart = currentChart;
    currentChart.on("click", (event: unknown) => {
      const item = event as {
        componentType?: string;
        data?: { spike?: TrafficSpike; spikeSource?: TrafficSpikeSource };
      };
      if (item.componentType !== "markPoint" || !item.data?.spike) return;
      const url = spikePublicUrl(item.data.spikeSource?.publicUrl);
      if (url) window.open(url, "_blank", "noopener,noreferrer");
      else inspectPeriod(periods.value.indexOf(item.data.spike.date));
    });
  }

  const colors = readTrafficChartTheme(chartEl.value);
  const interval = props.interval;
  const reduceMotion = prefersReducedMotion();
  const series = chartSeries(colors);
  const range = `${props.rangeKey}:${props.interval}:${periods.value[0]}:${periods.value.at(-1)}`;
  const rangeChanged = renderedRange !== range;
  const zoom = chartZoom(periods.value.length, colors).map((option) => {
    // Let a manual zoom survive refreshes within the same date range.
    const { start, end, rangeMode, ...appearance } = option;
    return rangeChanged ? option : appearance;
  });

  currentChart.setOption(
    {
      ...chartMotion(reduceMotion, 680),
      aria: {
        description: chartLabel.value,
        enabled: true,
      },
      color: colors.series,
      dataZoom: zoom,
      grid: chartPlotGrid(true),
      legend: {
        ...chartLegend(colors),
        selectedMode: false,
        data: props.metrics.map((metric) => trafficMetricLabels[metric]),
      },
      series,
      tooltip: {
        axisPointer: {
          type: "line",
          snap: true,
          label: {
            formatter: ({ value }: { value: unknown }) =>
              trafficPeriodLabel(String(value), interval),
          },
          lineStyle: {
            color: colors.axis,
          },
        },
        backgroundColor: colors.tooltipBg,
        borderColor: colors.grid,
        borderWidth: 1,
        padding: 14,
        borderRadius: 12,
        confine: true,
        enterable: true,
        extraCssText:
          "max-width:min(340px,calc(100vw - 48px));white-space:normal;line-height:1.5;overflow-wrap:anywhere;",
        formatter: tooltipContent,
        textStyle: {
          color: colors.tooltipText,
          fontFamily: "Geist Variable, sans-serif",
        },
        trigger: "axis",
      },
      xAxis: {
        axisLabel: {
          color: colors.text,
          hideOverlap: true,
          formatter: (value: string) =>
            trafficPeriodLabel(value, interval, true),
        },
        axisLine: {
          lineStyle: {
            color: colors.axis,
          },
        },
        axisTick: {
          show: false,
        },
        boundaryGap: false,
        data: periods.value,
        type: "category",
      },
      yAxis: {
        axisLabel: {
          color: colors.text,
        },
        axisLine: {
          show: false,
        },
        axisTick: {
          show: false,
        },
        minInterval: 1,
        splitLine: {
          lineStyle: {
            color: colors.grid,
          },
        },
        type: "value",
      },
    } satisfies OverviewChartOption,
    {
      lazyUpdate: false,
      notMerge: false,
      replaceMerge: ["series"],
      silent: false,
    },
  );
  if (rangeChanged)
    currentChart.dispatchAction({ type: "dataZoom", start: 0, end: 100 });
  renderedRange = range;
}

function tooltipContent(input: unknown): string {
  const entries = (Array.isArray(input) ? input : [input]) as {
    name?: string;
    data?: { spike?: TrafficSpike; spikeSource?: TrafficSpikeSource };
  }[];
  const markerData = entries.find((entry) => entry.data?.spike)?.data;
  const marker = markerData?.spike;
  const date = marker?.date ?? entries[0]?.name ?? "";
  const partial = incompleteTrafficPeriod(date, props.interval, props.asOf);
  const heading = chartText(trafficPeriodLabel(date, props.interval));
  const values = props.metrics.map((metric) => {
    const value = trafficByDate.value.get(date)?.[metric];
    return (
      chartText(trafficMetricLabels[metric]) +
      ": <strong>" +
      (value == null ? "Unavailable" : chartText(value.toLocaleString())) +
      "</strong>"
    );
  });
  const spike =
    marker ?? visibleSpikes.value.find((item) => item.date === date);
  if (spike) {
    values.push(
      "Eligible page views: " +
        chartText(spike.observed.toLocaleString()) +
        " · expected " +
        chartText(Math.round(spike.expected).toLocaleString()),
    );
    const sources = markerData?.spikeSource
      ? [markerData.spikeSource]
      : spike.sources;
    for (const source of sources) {
      let line =
        "<strong>" +
        chartText(source.label) +
        "</strong>: " +
        chartText(source.observed.toLocaleString()) +
        " (" +
        chartText(
          source.share.toLocaleString(undefined, { maximumFractionDigits: 1 }),
        ) +
        "%)";
      if (source.publicUrl)
        line +=
          ' · <a style="color:inherit" target="_blank" rel="noopener noreferrer" href="' +
          chartText(source.publicUrl) +
          '" title="' +
          chartText(source.publicUrl) +
          '">Post ↗</a>' +
          (markerData?.spikeSource ? "<br>" + chartText(source.publicUrl) : "");
      values.push(line);
    }
    values.push(
      "Source shares use eligible page views, excluding known crawlers.",
    );
    values.push(
      "Traffic totals above use a broader population. Association, not proven causation.",
    );
  }
  return (
    "<strong>" +
    heading +
    "</strong>" +
    (partial ? "<br>Partial period · still collecting" : "") +
    "<br>" +
    values.join("<br>")
  );
}

function inspectPeriod(index: number) {
  focusedIndex.value = Math.max(0, Math.min(periods.value.length - 1, index));
  const chart = ensureChart();
  chart?.dispatchAction({
    type: "showTip",
    seriesIndex: incompleteTrafficPeriod(
      periods.value[focusedIndex.value]!,
      props.interval,
      props.asOf,
    )
      ? 1
      : 0,
    dataIndex: focusedIndex.value,
  });
}
function keyboardPeriod(event: KeyboardEvent) {
  if (event.key === "Escape") {
    focusedIndex.value = null;
    ensureChart()?.dispatchAction({ type: "hideTip" });
    return;
  }
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  inspectPeriod(
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? periods.value.length - 1
        : (focusedIndex.value ?? 0) + (event.key === "ArrowRight" ? 1 : -1),
  );
}

function chartSeries(colors: ReturnType<typeof readTrafficChartTheme>) {
  return props.metrics.flatMap((metric) => {
    const color =
      metric === "sessions" ? colors.series[3] : colors.traffic[metric];
    const values = periods.value.map(
      (date) => trafficByDate.value.get(date)?.[metric] ?? null,
    );
    const segments = trafficLineSegments(
      periods.value,
      values,
      props.interval,
      props.asOf,
    );
    const base: LineSeriesOption = {
      ...chartLineShape,
      emphasis: { focus: "series" },
      showSymbol: false,
      symbol: "circle",
      symbolSize: 8,
      type: "line",
      itemStyle: { color },
      lineStyle: {
        ...chartLineStroke,
        color,
        shadowBlur: metric === "pageviews" ? 14 : 10,
        shadowColor: color,
        width: metric === "pageviews" ? 3.25 : 2.75,
      },
      name: trafficMetricLabels[metric],
    };
    const complete: LineSeriesOption = {
      ...base,
      id: "traffic-" + metric,
      data: periods.value.map((date, index) => ({
        name: date,
        value: segments.complete[index],
      })),
      ...(metric === "pageviews" && visibleSpikes.value.length
        ? {
            markPoint: {
              symbolSize: 24,
              symbolOffset: [0, -20],
              label: { show: false },
              data: visibleSpikes.value.flatMap((spike) =>
                spike.sources.flatMap((source, index) => {
                  const icon = referrerIconSource(
                    source.sourceDomain ?? source.label,
                  );
                  const point = {
                    name: spike.date,
                    coord: [
                      spike.date,
                      trafficByDate.value.get(spike.date)?.pageviews ??
                        spike.observed,
                    ],
                    // Separate the same day's source badges without inventing extra dates.
                    symbolOffset: [
                      (index - (spike.sources.length - 1) / 2) * 28,
                      -20,
                    ],
                    spike,
                    spikeSource: source,
                  };
                  if (!icon)
                    return [
                      {
                        ...point,
                        symbol: "pin",
                        symbolSize: 28,
                        itemStyle: {
                          color: colors.traffic.pageviews,
                          borderColor: colors.tooltipBg,
                          borderWidth: 2,
                        },
                      },
                    ];
                  return [
                    {
                      ...point,
                      symbol: "circle",
                      symbolSize: 30,
                      itemStyle: {
                        color: "#fffef9",
                        borderColor: colors.traffic.pageviews,
                        borderWidth: 2,
                      },
                    },
                    { ...point, symbol: "image://" + icon, symbolSize: 20 },
                  ];
                }),
              ),
              tooltip: { formatter: tooltipContent },
            },
          }
        : {}),
    };
    if (!segments.hasPartial) return [complete];
    return [
      complete,
      {
        ...base,
        id: "traffic-" + metric + "-partial",
        lineStyle: { ...base.lineStyle, type: "dashed", shadowBlur: 0 },
        data: periods.value.map((date, index) => ({
          name: date,
          value: segments.partial[index],
        })),
      } satisfies LineSeriesOption,
    ];
  });
}

async function captureSnapshot() {
  await nextTick();
  if (props.loading || props.error || !hasPoints.value)
    throw new Error("Traffic is not available for export.");
  renderChart();
  const chart = ensureChart();
  if (!chart) throw new Error("Chart is not ready.");
  return chartSnapshot(chart);
}
defineExpose({ captureSnapshot });

function trafficValue(date: string, metric: TrafficMetric) {
  return trafficByDate.value.get(date)?.[metric] ?? "—";
}
</script>

<template>
  <div class="chart-frame">
    <div v-if="loading && !hasPoints" class="chart-state" role="status">
      <span class="state-orbit" aria-hidden="true"></span>
      <strong>Loading traffic</strong>
      <span>Reading the selected site and date range.</span>
    </div>

    <div v-else-if="error" class="chart-state error" role="alert">
      <span class="state-symbol" aria-hidden="true">!</span>
      <strong>Traffic is unavailable</strong>
      <span>{{ error }}</span>
      <button class="text-button" type="button" @click="emit('retry')">
        Try again
      </button>
    </div>

    <div v-else-if="!hasPoints" class="chart-state" role="status">
      <span class="state-symbol" aria-hidden="true">—</span>
      <strong>No traffic in this range</strong>
      <span
        >New page views and events will appear here after the SDK sends
        them.</span
      >
    </div>

    <div
      v-else-if="hasPoints"
      ref="chartEl"
      class="chart-surface"
      :class="{ 'is-updating': loading }"
      role="img"
      :aria-busy="loading"
      :aria-label="chartLabel"
      tabindex="0"
      @keydown="keyboardPeriod"
      @focus="inspectPeriod(focusedIndex ?? periods.length - 1)"
      @blur="focusedIndex = null"
    ></div>

    <p v-if="hasPartial && hasPoints && !error" class="traffic-chart-note">
      Dashed segment: unfinished period (UTC). Hover, tap or use arrow keys for
      values.
    </p>
    <p v-else-if="hasPoints && !error" class="traffic-chart-note">
      Hover, tap or use arrow keys for values.
    </p>
    <div
      v-if="visibleSpikes.length && !error"
      class="traffic-spikes"
      aria-label="Unusual traffic sources"
    >
      <div
        v-for="spike in visibleSpikes"
        :key="spike.date"
        class="traffic-spike-day"
      >
        <span class="traffic-spike-date">{{
          trafficPeriodLabel(spike.date, "day", true)
        }}</span>
        <component
          v-for="(source, sourceIndex) in spike.sources"
          :key="sourceIndex"
          :is="source.publicUrl ? 'a' : 'button'"
          :href="source.publicUrl ?? undefined"
          :type="source.publicUrl ? undefined : 'button'"
          :target="source.publicUrl ? '_blank' : undefined"
          :rel="source.publicUrl ? 'noopener noreferrer' : undefined"
          :title="`${source.label}: ${source.observed.toLocaleString()} eligible page views (${source.share.toLocaleString(undefined, { maximumFractionDigits: 1 })}%)${source.publicUrl ? ' · Owner-provided post: ' + source.publicUrl : ''}`"
          class="traffic-spike"
          @mouseenter="inspectPeriod(periods.indexOf(spike.date))"
          @focus="inspectPeriod(periods.indexOf(spike.date))"
          @click="
            !source.publicUrl && inspectPeriod(periods.indexOf(spike.date))
          "
        >
          <ReferrerIcon :source="source.sourceDomain ?? source.label" />
          <span
            >{{ source.label }} · {{ source.observed.toLocaleString() }} ({{
              source.share.toLocaleString(undefined, {
                maximumFractionDigits: 1,
              })
            }}%)<span v-if="source.publicUrl" aria-hidden="true"> ↗</span></span
          >
        </component>
      </div>
      <span class="traffic-chart-note"
        >Associated sources · not proven causes. Post links are
        owner-provided.</span
      >
    </div>
    <span v-if="focusedIndex !== null" class="sr-only" aria-live="polite"
      >{{ trafficPeriodLabel(periods[focusedIndex ?? 0]!, interval) }}.
      {{
        metrics
          .map(
            (metric) =>
              trafficMetricLabels[metric] +
              ": " +
              trafficValue(periods[focusedIndex ?? 0]!, metric),
          )
          .join(". ")
      }}</span
    >
    <p v-if="loading && hasPoints" class="sr-only" role="status">
      Updating traffic for the selected date range.
    </p>

    <table v-if="hasPoints && !loading && !error" class="sr-only">
      <caption>
        Traffic data
      </caption>
      <thead>
        <tr>
          <th scope="col">Period</th>
          <th v-for="metric in metrics" :key="metric" scope="col">
            {{ trafficMetricLabels[metric] }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="period in periods" :key="period">
          <th scope="row">{{ trafficPeriodLabel(period, interval) }}</th>
          <td v-for="metric in metrics" :key="metric">
            {{ trafficValue(period, metric) }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.traffic-chart-note {
  margin: 8px 0 0;
  font-size: 0.8rem;
  color: var(--chart-text, var(--text-secondary));
  line-height: 1.5;
}
.chart-surface:focus-visible {
  outline: 2px solid var(--chart-2, #a391ff);
  outline-offset: 4px;
  border-radius: 12px;
}
.traffic-spikes {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}
.traffic-spike-day {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  max-width: 100%;
}
.traffic-spike-date {
  font-size: 0.8rem;
  color: var(--chart-text, var(--text-secondary));
}
.traffic-spike {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border: 1px solid var(--chart-grid, #555861);
  border-radius: 999px;
  color: var(--chart-tooltip-text, #f7f7f2);
  background: var(--chart-tooltip-bg, #18191d);
  font: inherit;
  font-size: 0.85rem;
  max-width: 100%;
  overflow-wrap: anywhere;
  text-decoration: none;
  cursor: pointer;
}
.traffic-spike:hover,
.traffic-spike:focus-visible {
  border-color: var(--chart-1, #d8ff52);
  outline: 2px solid var(--chart-2, #a391ff);
  outline-offset: 2px;
}
</style>
