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
import type { PerformanceMetric, PerformancePoint } from "~/types/insights";
import { chartMotion, chartZoom, readChartTheme } from "~/utils/chartTheme";

type Option = ComposeOption<
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
    loading?: boolean;
    metric: PerformanceMetric;
    points?: PerformancePoint[];
    emptyDescription?: string;
  }>(),
  {
    loading: false,
    points: () => [],
    emptyDescription:
      "Try a wider date range or check Web Vitals collection in the SDK.",
  },
);
const chartEl = ref<HTMLElement | null>(null);
const { clearChart, ensureChart, prefersReducedMotion } = useEChartLifecycle(
  chartEl,
  renderChart,
  { observeTheme: true },
);

watch(
  () => [props.metric, props.points, props.loading] as const,
  async () => {
    await nextTick();
    if (!props.points.length) clearChart();
    else renderChart();
  },
  { deep: true },
);
onMounted(async () => {
  await nextTick();
  renderChart();
});

function renderChart() {
  if (!chartEl.value || !props.points.length) return;
  const chart = ensureChart();
  if (!chart) return;
  const theme = readChartTheme(chartEl.value);
  const colors = ["#b95300", "#ff842b", "#ffbd84"];
  const values = (key: "p50" | "p75" | "p95") =>
    props.points.map((point) => point[key]);
  const series = (["p50", "p75", "p95"] as const).map((key, index) => ({
    areaStyle: key === "p75" ? { color: "rgba(255, 132, 43, 0.1)" } : undefined,
    data: values(key),
    emphasis: { focus: "series" as const },
    itemStyle: { color: colors[index] },
    lineStyle: {
      color: colors[index],
      shadowBlur: key === "p75" ? 12 : 4,
      shadowColor: colors[index],
      type: key === "p95" ? ("dotted" as const) : ("solid" as const),
      width: key === "p75" ? 3 : 2,
    },
    name: key,
    showSymbol: props.points.length === 1,
    symbolSize: 7,
    smooth: 0.34,
    type: "line" as const,
  })) satisfies LineSeriesOption[];

  chart.setOption(
    {
      ...chartMotion(prefersReducedMotion(), 620),
      aria: {
        description: `${props.metric} field measurements over ${props.points.length} days, with p50, p75, and p95 percentiles.`,
        enabled: true,
      },
      dataZoom: chartZoom(props.points.length, theme),
      grid: { bottom: 112, containLabel: true, left: 8, right: 14, top: 24 },
      legend: {
        bottom: 0,
        itemHeight: 8,
        itemWidth: 16,
        textStyle: {
          color: theme.text,
          fontFamily: "Geist Variable, sans-serif",
        },
      },
      series,
      tooltip: {
        backgroundColor: theme.tooltipBg,
        borderColor: theme.grid,
        textStyle: {
          color: theme.tooltipText,
          fontFamily: "Geist Variable, sans-serif",
        },
        trigger: "axis",
        valueFormatter: (value) => formatValue(Number(value)),
      },
      xAxis: {
        axisLabel: { color: theme.text, hideOverlap: true },
        axisLine: { lineStyle: { color: theme.axis } },
        axisTick: { show: false },
        boundaryGap: false,
        data: props.points.map((point) => point.date),
        type: "category",
      },
      yAxis: {
        axisLabel: {
          color: theme.text,
          formatter: (value: number) => formatValue(value),
        },
        axisLine: { show: false },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: theme.grid, type: "dashed" } },
        type: "value",
      },
    } satisfies Option,
    { notMerge: true },
  );
}

function formatValue(value: number) {
  return props.metric === "CLS" ? value.toFixed(3) : `${Math.round(value)} ms`;
}
</script>

<template>
  <div class="performance-chart-shell">
    <div
      v-if="points.length"
      ref="chartEl"
      class="performance-chart"
      :class="{ updating: loading }"
      role="img"
      :aria-label="`${metric} percentile history`"
    ></div>
    <div v-else-if="loading" class="chart-state" role="status">
      <strong>Loading {{ metric }} measurements…</strong>
    </div>
    <div v-else class="chart-state">
      <span class="state-symbol">—</span>
      <strong>No {{ metric }} samples in this range</strong>
      <span>{{ emptyDescription }}</span>
    </div>
  </div>
</template>

<style scoped>
.performance-chart-shell,
.performance-chart {
  min-height: 430px;
}
.performance-chart {
  width: 100%;
  height: 430px;
  transition: opacity 140ms ease;
}
.performance-chart.updating {
  opacity: 0.6;
}
</style>
