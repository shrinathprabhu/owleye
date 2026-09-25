<script setup lang="ts">
import { LineChart } from "echarts/charts";
import {
  AriaComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import {
  publicMetricLabels,
  type PublicOverview,
  type PublicMetric,
} from "~/types/publicDashboard";
use([
  LineChart,
  AriaComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer,
]);
const props = defineProps<{
  points: NonNullable<PublicOverview["data"]["traffic"]>;
  metrics: PublicMetric[];
}>();
const chartEl = ref<HTMLElement | null>(null);
const { ensureChart } = useEChartLifecycle(chartEl, render);
function render() {
  ensureChart()?.setOption(
    {
      animation: false,
      color: ["#4255ed", "#6b8b26", "#ad5e37", "#7858b8"],
      aria: { enabled: true },
      tooltip: { trigger: "axis", renderMode: "richText", confine: true },
      legend: { bottom: 0 },
      grid: { top: 20, left: 12, right: 16, bottom: 50, containLabel: true },
      xAxis: {
        type: "category",
        data: props.points.map((p) => p.date),
        axisLabel: { hideOverlap: true },
      },
      yAxis: { type: "value", min: 0, minInterval: 1 },
      series: props.metrics.map((metric) => ({
        name: publicMetricLabels[metric],
        type: "line",
        showSymbol: false,
        data: props.points.map((p) => p[metric] ?? null),
      })),
    },
    { notMerge: true },
  );
}
onMounted(render);
watch(
  () => [props.points, props.metrics],
  () => nextTick(render),
);
</script>
<template>
  <div
    ref="chartEl"
    class="public-traffic-chart"
    role="img"
    aria-label="Daily traffic for the shared metrics. Exact values are available in the daily data table below."
  ></div>
</template>
<style scoped>
.public-traffic-chart {
  height: 320px;
  width: 100%;
  min-width: 0;
}
</style>
