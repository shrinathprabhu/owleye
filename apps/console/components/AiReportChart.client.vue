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
import {
  chartBarCorners,
  chartLegend,
  chartLineShape,
  chartLineStroke,
  chartPlotGrid,
  chartRingItemStyle,
  readChartTheme,
  readCssVariable,
} from "~/utils/chartTheme";
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
const { ensureChart } = useEChartLifecycle(chartEl, render, {
  observeTheme: true,
});
const points = computed(() => chartPoints(props.evidence, props.metric));
const label = computed(
  () =>
    `${props.kind} chart of ${metricLabels[props.metric].toLowerCase()}, ${props.evidence.start_date} to ${props.evidence.end_date} UTC. Supporting data follows.`,
);
function seriesData(values: (number | null)[]) {
  return props.kind === "bar"
    ? values.map((value) => ({
        value,
        itemStyle: { borderRadius: chartBarCorners(value ?? 0) },
      }))
    : values;
}
function render() {
  if (!chartEl.value) return;
  const chart = ensureChart();
  if (!chart) return;
  const theme = readChartTheme(chartEl.value);
  const round = props.kind === "pie" || props.kind === "donut";
  chart.setOption(
    {
      animation: false,
      backgroundColor: "transparent",
      color: theme.series,
      textStyle: { fontFamily: "Geist Variable, sans-serif" },
      aria: { enabled: true, description: label.value },
      tooltip: {
        trigger: round ? "item" : "axis",
        renderMode: "richText",
        confine: true,
        backgroundColor: theme.tooltipBg,
        borderColor: theme.grid,
        borderRadius: 12,
        borderWidth: 1,
        textStyle: {
          color: theme.tooltipText,
          fontFamily: "Geist Variable, sans-serif",
        },
      },
      legend: round ? chartLegend(theme) : undefined,
      grid: round ? undefined : chartPlotGrid(),
      xAxis: round
        ? undefined
        : {
            type: "category",
            boundaryGap: props.kind === "bar",
            axisLine: { lineStyle: { color: theme.axis } },
            axisTick: { show: false },
            data: points.value.map((p) => p.label),
            axisLabel: {
              color: theme.text,
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
            axisLabel: { color: theme.text },
            axisLine: { show: false },
            axisTick: { show: false },
            splitLine: { lineStyle: { color: theme.grid } },
          },
      series: round
        ? [
            {
              type: "pie",
              name: metricLabels[props.metric],
              radius: props.kind === "donut" ? ["42%", "68%"] : "68%",
              center: ["50%", "44%"],
              label: { show: false },
              itemStyle: chartRingItemStyle(theme),
              stillShowZeroSum: false,
              data: points.value
                .filter((p) => p.value !== null)
                .map((p) => ({ name: p.label, value: p.value! })),
            },
          ]
        : [
            {
              ...(props.kind === "bar" ? {} : chartLineShape),
              type: props.kind === "bar" ? "bar" : "line",
              name: metricLabels[props.metric],
              data: seriesData(points.value.map((p) => p.value)),
              connectNulls: false,
              lineStyle: { ...chartLineStroke },
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
        backgroundColor: readCssVariable(
          chartEl.value,
          "--owleye-night",
          "#111214",
        ),
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
    class="chart-panel-surface ai-report-chart"
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
