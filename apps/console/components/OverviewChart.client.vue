<script setup lang="ts">
import { LineChart, type LineSeriesOption } from "echarts/charts";
import {
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  type AriaComponentOption,
  type DataZoomComponentOption,
  type GridComponentOption,
  type LegendComponentOption,
  type TooltipComponentOption,
} from "echarts/components";
import { use, type ComposeOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

import { useEChartLifecycle } from "~/composables/useEChartLifecycle";
import {
  trafficPeriodLabel,
  type AnalyticsInterval,
} from "~/utils/analyticsRange";
import type { TrafficPoint } from "~/types/stats";
import {
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
  LineChart,
  TooltipComponent,
]);

const props = withDefaults(
  defineProps<{
    interval?: AnalyticsInterval;
    rangeKey?: string;
    error?: string;
    loading?: boolean;
    points?: TrafficPoint[];
  }>(),
  {
    interval: "day",
    rangeKey: "",
    error: "",
    loading: false,
    points: () => [],
  },
);

const emit = defineEmits<{
  retry: [];
}>();

let renderedRange = "";
const chartEl = ref<HTMLElement | null>(null);
const periods = computed(() =>
  props.points
    .map((point) => point.date)
    .sort((left, right) => left.localeCompare(right)),
);
const hasPoints = computed(() => periods.value.length > 0);
const trafficByDate = computed(
  () => new Map(props.points.map((point) => [point.date, point] as const)),
);
const chartLabel = computed(
  () =>
    `Traffic for ${periods.value.length} time periods. Page views, unique visitors, and tracked events are shown as three lines.`,
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
      props.rangeKey,
      props.interval,
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

  const colors = readTrafficChartTheme(chartEl.value);
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
      grid: {
        bottom: 112,
        left: 8,
        outerBoundsContain: "axisLabel",
        outerBoundsMode: "same",
        right: 12,
        top: 20,
      },
      legend: {
        bottom: 0,
        itemHeight: 8,
        itemWidth: 18,
        textStyle: {
          color: colors.text,
          fontFamily: "Geist Variable, sans-serif",
        },
      },
      series,
      tooltip: {
        axisPointer: {
          label: {
            formatter: ({ value }: { value: unknown }) =>
              trafficPeriodLabel(String(value), props.interval),
          },
          lineStyle: {
            color: colors.axis,
          },
        },
        backgroundColor: colors.tooltipBg,
        borderColor: colors.grid,
        borderWidth: 1,
        padding: 10,
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
            trafficPeriodLabel(value, props.interval, true),
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

function chartSeries(colors: ReturnType<typeof readTrafficChartTheme>) {
  const dataFor = (metric: keyof Omit<TrafficPoint, "date">) =>
    periods.value.map((date) => ({
      name: date,
      value: trafficByDate.value.get(date)?.[metric] ?? null,
    }));
  const shared = {
    emphasis: { focus: "series" as const },
    showSymbol: false,
    smooth: 0.36,
    smoothMonotone: "x" as const,
    type: "line" as const,
  };
  return [
    {
      ...shared,
      data: dataFor("pageviews"),
      id: "traffic-pageviews",
      itemStyle: { color: colors.traffic.pageviews },
      lineStyle: {
        cap: "round",
        color: colors.traffic.pageviews,
        join: "round",
        shadowBlur: 14,
        shadowColor: colors.traffic.pageviews,
        width: 3.25,
      },
      name: "Page views",
    },
    {
      ...shared,
      data: dataFor("visitors"),
      id: "traffic-visitors",
      itemStyle: { color: colors.traffic.visitors },
      lineStyle: {
        cap: "round",
        color: colors.traffic.visitors,
        join: "round",
        shadowBlur: 11,
        shadowColor: colors.traffic.visitors,
        width: 2.75,
      },
      name: "Unique visitors",
    },
    {
      ...shared,
      data: dataFor("events"),
      id: "traffic-events",
      itemStyle: { color: colors.traffic.events },
      lineStyle: {
        cap: "round",
        color: colors.traffic.events,
        join: "round",
        shadowBlur: 10,
        shadowColor: colors.traffic.events,
        width: 2.75,
      },
      name: "Events",
    },
  ] satisfies LineSeriesOption[];
}

function trafficValue(date: string, metric: keyof Omit<TrafficPoint, "date">) {
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
    ></div>

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
          <th scope="col">Page views</th>
          <th scope="col">Unique visitors</th>
          <th scope="col">Events</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="period in periods" :key="period">
          <th scope="row">{{ trafficPeriodLabel(period, interval) }}</th>
          <td>{{ trafficValue(period, "pageviews") }}</td>
          <td>{{ trafficValue(period, "visitors") }}</td>
          <td>{{ trafficValue(period, "events") }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
