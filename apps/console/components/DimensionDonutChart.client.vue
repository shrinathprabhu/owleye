<script setup lang="ts">
import { PieChart, type PieSeriesOption } from "echarts/charts";
import {
  AriaComponent,
  LegendComponent,
  TitleComponent,
  TooltipComponent,
  type AriaComponentOption,
  type LegendComponentOption,
  type TitleComponentOption,
  type TooltipComponentOption,
} from "echarts/components";
import { use, type ComposeOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

import { useEChartLifecycle } from "~/composables/useEChartLifecycle";
import type { DimensionStat } from "~/types/stats";
import {
  chartMotion,
  readChartTheme,
  readCssVariable,
} from "~/utils/chartTheme";

type DonutChartOption = ComposeOption<
  | AriaComponentOption
  | LegendComponentOption
  | PieSeriesOption
  | TitleComponentOption
  | TooltipComponentOption
>;

use([
  AriaComponent,
  CanvasRenderer,
  LegendComponent,
  PieChart,
  TitleComponent,
  TooltipComponent,
]);

const props = withDefaults(
  defineProps<{
    emptyMessage?: string;
    error?: string;
    items?: DimensionStat[];
    label: string;
    loading?: boolean;
    metricLabel?: string;
    total?: number;
  }>(),
  {
    emptyMessage: "No data in this range.",
    error: "",
    items: () => [],
    loading: false,
    metricLabel: "Page views",
  },
);

const emit = defineEmits<{
  retry: [];
}>();

const chartEl = ref<HTMLElement | null>(null);
const chartItems = computed(() =>
  [...props.items]
    .filter((item) => item.count > 0)
    .sort((left, right) => right.count - left.count),
);
const hasItems = computed(() => chartItems.value.length > 0);
const total = computed(
  () =>
    props.total ?? chartItems.value.reduce((sum, item) => sum + item.count, 0),
);
const chartLabel = computed(() => {
  const summary = chartItems.value
    .map((item) => `${item.name}: ${item.count.toLocaleString()}`)
    .join(", ");

  return `${props.label}. Donut chart with ${total.value.toLocaleString()} total ${props.metricLabel.toLowerCase()}. ${summary}.`;
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
  () =>
    [
      props.error,
      props.items,
      props.label,
      props.loading,
      props.metricLabel,
      props.total,
    ] as const,
  async () => {
    await nextTick();
    if (!hasItems.value || props.error) {
      clearChart();
      return;
    }
    if (!props.loading) renderChart();
  },
);

function renderChart() {
  if (!chartEl.value || !hasItems.value || props.error) return;
  const currentChart = ensureChart();
  if (!currentChart) return;

  const colors = readChartTheme(chartEl.value);
  const panelColor = readCssVariable(
    chartEl.value,
    "--owleye-night",
    "#111214",
  );
  const donutColors = [
    readCssVariable(chartEl.value, "--accent", "#d8ff52"),
    readCssVariable(chartEl.value, "--brand", "#3d5afe"),
    readCssVariable(chartEl.value, "--coral", "#ff705c"),
    readCssVariable(chartEl.value, "--violet", "#a391ff"),
    "#a9d637",
    "#7188ff",
    "#ff9b8b",
    "#c2b8ff",
  ];
  const reduceMotion = prefersReducedMotion();

  currentChart.setOption(
    {
      ...chartMotion(reduceMotion, 620),
      aria: { description: chartLabel.value, enabled: true },
      color: donutColors,
      legend: {
        bottom: 0,
        icon: "circle",
        itemHeight: 9,
        itemWidth: 9,
        pageIconColor: colors.text,
        pageIconInactiveColor: colors.axis,
        pageTextStyle: { color: colors.text },
        textStyle: {
          color: colors.text,
          fontFamily: "Geist Variable, sans-serif",
        },
        type: "scroll",
      },
      series: [
        {
          avoidLabelOverlap: true,
          center: ["50%", "43%"],
          data: chartItems.value.map((item) => ({
            name: item.name,
            value: item.count,
          })),
          emphasis: {
            itemStyle: {
              shadowBlur: 12,
              shadowColor: "rgba(17, 18, 20, 0.18)",
            },
            scaleSize: 6,
          },
          itemStyle: {
            borderColor: panelColor,
            borderRadius: 8,
            borderWidth: 3,
          },
          label: { show: false },
          labelLine: { show: false },
          minAngle: 2,
          name: props.label,
          radius: ["50%", "73%"],
          stillShowZeroSum: false,
          type: "pie",
        },
      ],
      title: {
        left: "center",
        subtext: props.metricLabel.toLowerCase(),
        subtextStyle: {
          color: colors.text,
          fontFamily: "Geist Mono Variable, monospace",
          fontSize: 11,
        },
        text: compactNumber(total.value),
        textStyle: {
          color: "#fffef9",
          fontFamily: "Geist Variable, sans-serif",
          fontSize: 24,
          fontWeight: 760,
        },
        top: "34%",
      },
      tooltip: {
        backgroundColor: colors.tooltipBg,
        borderColor: colors.grid,
        borderWidth: 1,
        formatter: (rawParams: unknown) => {
          const { marker, name, percent, value } =
            rawParams as DonutTooltipParams;
          return `${marker}${escapeHtml(name)}<br><strong>${Number(value).toLocaleString()}</strong> ${props.metricLabel.toLowerCase()}${props.metricLabel === "Page views" ? ` · ${percent}%` : ""}`;
        },
        textStyle: {
          color: colors.tooltipText,
          fontFamily: "Geist Variable, sans-serif",
        },
        trigger: "item",
      },
    } satisfies DonutChartOption,
    {
      lazyUpdate: false,
      notMerge: false,
      replaceMerge: ["series"],
    },
  );
}

type DonutTooltipParams = {
  marker: string;
  name: string;
  percent: number;
  value: number;
};

function compactNumber(value: number) {
  return new Intl.NumberFormat("en", {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(value);
}

function escapeHtml(value: string) {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        '"': "&quot;",
        "'": "&#39;",
        "<": "&lt;",
        ">": "&gt;",
      })[character] || character,
  );
}
</script>

<template>
  <div class="chart-frame dimension-donut-frame">
    <div v-if="loading && !hasItems" class="chart-state" role="status">
      <span class="state-orbit" aria-hidden="true"></span>
      <strong>Loading {{ label.toLocaleLowerCase() }}</strong>
      <span>Reading the selected site and date range.</span>
    </div>

    <div v-else-if="error" class="chart-state error" role="alert">
      <span class="state-symbol" aria-hidden="true">!</span>
      <strong>{{ label }} is unavailable</strong>
      <span>{{ error }}</span>
      <button class="text-button" type="button" @click="emit('retry')">
        Try again
      </button>
    </div>

    <div v-else-if="!hasItems" class="chart-state" role="status">
      <span class="state-symbol" aria-hidden="true">—</span>
      <strong>No {{ label.toLocaleLowerCase() }}</strong>
      <span>{{ emptyMessage }}</span>
    </div>

    <div
      v-else
      ref="chartEl"
      class="chart-surface dimension-donut-surface"
      :class="{ 'is-updating': loading }"
      role="img"
      :aria-busy="loading"
      :aria-label="chartLabel"
    ></div>

    <p v-if="loading && hasItems" class="sr-only" role="status">
      Updating {{ label.toLocaleLowerCase() }} for the selected date range.
    </p>

    <table v-if="hasItems && !loading && !error" class="sr-only">
      <caption>
        {{
          label
        }}
        {{
          metricLabel.toLowerCase()
        }}
        totals
      </caption>
      <thead>
        <tr>
          <th scope="col">{{ label }}</th>
          <th scope="col">{{ metricLabel }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="item in chartItems" :key="item.name">
          <th scope="row">{{ item.name }}</th>
          <td>{{ item.count }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.dimension-donut-frame {
  --chart-axis: #707279;
  --chart-grid: #34363d;
  --chart-text: #c4c5c0;
  --chart-tooltip-bg: #f7f7f2;
  --chart-tooltip-text: #111214;

  min-height: 320px;
  overflow: hidden;
  border: 1.5px solid #303137;
  border-radius: var(--radius-xl, 22px);
  color: #fffef9;
  background:
    radial-gradient(circle at 78% 10%, rgb(61 90 254 / 0.18), transparent 38%),
    #111214;
}

.dimension-donut-surface {
  height: 320px;
  min-height: 320px;
}

.dimension-donut-frame :deep(.chart-state) {
  min-height: 320px;
  color: #fffef9;
}

.dimension-donut-frame :deep(.chart-state span) {
  color: #aeb0b8;
}

@media (max-width: 520px) {
  .dimension-donut-frame,
  .dimension-donut-surface {
    min-height: 290px;
  }

  .dimension-donut-surface {
    height: 290px;
  }
}
</style>
