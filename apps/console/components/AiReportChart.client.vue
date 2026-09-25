<script setup lang="ts">
import { BarChart, LineChart, PieChart } from "echarts/charts";
import {
  AriaComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import type { EChartsOption } from "echarts";
import type { AiEvidence, AiChart, AiMetric } from "~/types/ai";
import { chartPoints, metricLabels } from "~/utils/aiReport";
use([
  BarChart,
  LineChart,
  PieChart,
  AriaComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer,
]);
const props = defineProps<{
  evidence: AiEvidence;
  kind: AiChart;
  metric: AiMetric;
}>();
const chartEl = ref<HTMLElement | null>(null);
const { ensureChart } = useEChartLifecycle(chartEl, render);
const points = computed(() => chartPoints(props.evidence, props.metric));
const label = computed(
  () =>
    `${props.kind} chart of ${metricLabels[props.metric].toLowerCase()}, ${props.evidence.start_date} to ${props.evidence.end_date} UTC. Supporting data follows.`,
);
function render() {
  const chart = ensureChart();
  if (!chart) return;
  const round = props.kind === "pie" || props.kind === "donut";
  chart.setOption(
    {
      animation: false,
      backgroundColor: "#111214",
      color: ["#d8ff52", "#8295ff", "#ff9b8b", "#c2b8ff", "#66d9cc", "#ffc56b"],
      aria: { enabled: true, description: label.value },
      tooltip: {
        trigger: round ? "item" : "axis",
        renderMode: "richText",
        confine: true,
      },
      legend: round
        ? {
            type: "scroll",
            bottom: 0,
            textStyle: { color: "#d5d6dc" },
            pageTextStyle: { color: "#d5d6dc" },
          }
        : undefined,
      grid: round
        ? undefined
        : { top: 24, left: 12, right: 16, bottom: 12, containLabel: true },
      xAxis: round
        ? undefined
        : {
            type: "category",
            data: points.value.map((p) => p.label),
            axisLabel: {
              color: "#d5d6dc",
              width: 80,
              overflow: "truncate",
              rotate: points.value.length > 8 ? 35 : 0,
            },
          },
      yAxis: round
        ? undefined
        : {
            type: "value",
            min: 0,
            minInterval: 1,
            axisLabel: { color: "#d5d6dc" },
            splitLine: { lineStyle: { color: "#34363d" } },
          },
      series: round
        ? [
            {
              type: "pie",
              name: metricLabels[props.metric],
              radius: props.kind === "donut" ? ["42%", "68%"] : "68%",
              center: ["50%", "44%"],
              label: { show: false },
              stillShowZeroSum: false,
              data: points.value
                .filter((p) => p.value !== null)
                .map((p) => ({ name: p.label, value: p.value! })),
            },
          ]
        : [
            {
              type: props.kind === "bar" ? "bar" : "line",
              name: metricLabels[props.metric],
              data: points.value.map((p) => p.value),
              connectNulls: false,
              barMaxWidth: 44,
            },
          ],
    } as EChartsOption,
    { notMerge: true },
  );
}
onMounted(render);
watch(
  () => [props.evidence, props.kind, props.metric],
  () => nextTick(render),
);
defineExpose({
  snapshot: () => {
    const chart = ensureChart();
    if (!chart || !chartEl.value) return undefined;
    return {
      data: chart.getDataURL({
        type: "png",
        pixelRatio: 2,
        backgroundColor: "#111214",
      }),
      width: chart.getWidth(),
      height: chart.getHeight(),
    };
  },
});
</script>
<template>
  <div
    ref="chartEl"
    class="ai-report-chart"
    role="img"
    :aria-label="label"
  ></div>
</template>
<style scoped>
.ai-report-chart {
  width: 100%;
  min-width: 0;
  height: 320px;
}
@media (max-width: 520px) {
  .ai-report-chart {
    height: 280px;
  }
}
</style>
