<script setup lang="ts">
import { BarChart } from "echarts/charts";
import {
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  TooltipComponent,
} from "echarts/components";
import { use, type EChartsCoreOption } from "echarts/core";
import {
  countryColor,
  countryLabel,
  type CountryGeometry,
} from "~/utils/countryGeo";
import { CanvasRenderer } from "echarts/renderers";

import { useEChartLifecycle } from "~/composables/useEChartLifecycle";
import type { DimensionStat } from "~/types/stats";
import {
  COUNTRY_CODES_BY_NAME,
  normalizeCountryCode as toCountryCode,
} from "~/utils/countryMap";
import { chartMotion, readCssVariable } from "~/utils/chartTheme";

type CountryDatum = {
  code: string;
  count: number;
  label: string;
};

type GeoView = "bars" | "globe";

use([
  AriaComponent,
  DataZoomComponent,
  BarChart,
  CanvasRenderer,
  GridComponent,
  TooltipComponent,
]);

const props = withDefaults(
  defineProps<{
    countries?: DimensionStat[];
    error?: string;
    loading?: boolean;
    metricLabel?: string;
  }>(),
  {
    countries: () => [],
    error: "",
    loading: false,
    metricLabel: "Page views",
  },
);

const emit = defineEmits<{
  retry: [];
}>();

const chartEl = ref<HTMLElement | null>(null);
const view = ref<GeoView>("bars");
const globeReady = ref(false);
const globeLoading = ref(false);
const globeError = ref("");
const geometry = shallowRef<CountryGeometry | null>(null);
const countryLabels = ref<Record<string, string>>({});
const countryCodes = ref<Record<string, string>>({ ...COUNTRY_CODES_BY_NAME });

const hasCountries = computed(() => props.countries.length > 0);
const countryData = computed<CountryDatum[]>(() =>
  props.countries
    .filter((country) => country.count > 0)
    .map((country) => {
      const raw = country.name.trim();
      const code = normalizeCountryCode(raw);
      return {
        code,
        count: country.count,
        label: countryLabels.value[code] || countryLabel(code),
      };
    })
    .sort((left, right) => right.count - left.count),
);
const maxValue = computed(() =>
  Math.max(1, ...countryData.value.map((country) => country.count)),
);
const chartLabel = computed(() => {
  const summary = countryData.value
    .slice(0, 5)
    .map((country) => `${country.label}: ${country.count.toLocaleString()}`)
    .join(", ");
  return `${view.value === "globe" ? "Country globe" : "Country ranking"}. ${props.metricLabel}: ${summary}.`;
});

let globeController: AbortController | undefined;
let requestedView: GeoView = "bars";

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
    [props.countries, props.error, props.loading, props.metricLabel] as const,
  async () => {
    await nextTick();
    if (props.error || (!hasCountries.value && !props.loading)) {
      clearChart();
      return;
    }
    if (!props.loading) renderChart();
  },
);

watch(view, async () => {
  await nextTick();
  renderChart();
});

onBeforeUnmount(() => {
  globeController?.abort();
});

async function loadGlobe() {
  if (globeReady.value) return true;
  if (globeLoading.value) return false;
  globeError.value = "";
  globeLoading.value = true;

  const assetUrl = "/maps/world-countries.json";
  const controller = new AbortController();
  globeController = controller;

  try {
    const response = await fetch(assetUrl, { signal: controller.signal });
    if (!response.ok)
      throw new Error(`Globe request failed with ${response.status}`);
    const geo = (await response.json()) as CountryGeometry;
    geometry.value = geo;
    countryLabels.value = Object.fromEntries(
      geo.features.map((feature) => [
        feature.properties.name,
        feature.properties.label,
      ]),
    );
    countryCodes.value = {
      ...COUNTRY_CODES_BY_NAME,
      ...Object.fromEntries(
        geo.features.map((feature) => [
          feature.properties.label,
          feature.properties.name,
        ]),
      ),
    };
    globeReady.value = true;
    return true;
  } catch (error) {
    if (isAbortError(error)) return false;
    globeError.value =
      "Globe view could not be loaded. The ranked view still works.";
    globeReady.value = false;
    if (view.value === "globe") view.value = "bars";
    return false;
  } finally {
    if (globeController === controller) globeController = undefined;
    globeLoading.value = false;
  }
}

async function setView(nextView: GeoView) {
  requestedView = nextView;
  if (nextView !== "bars" && !globeReady.value && !(await loadGlobe())) return;
  if (requestedView !== nextView) return;
  view.value = nextView;
}

function renderChart() {
  if (
    !hasCountries.value ||
    props.loading ||
    props.error ||
    view.value === "globe"
  )
    return;
  const currentChart = ensureChart();
  if (!currentChart || !chartEl.value) return;

  const reduceMotion = prefersReducedMotion();
  const colors = geoColors(chartEl.value);
  const option = barOption(colors, reduceMotion);

  currentChart.setOption(option, {
    lazyUpdate: false,
    notMerge: true,
  });
}

function sharedOption(reduceMotion: boolean): EChartsCoreOption {
  return {
    ...chartMotion(reduceMotion, 760),
    aria: { description: chartLabel.value, enabled: true },
  };
}

function barOption(
  colors: ReturnType<typeof geoColors>,
  reduceMotion: boolean,
): EChartsCoreOption {
  return {
    ...sharedOption(reduceMotion),
    dataZoom:
      countryData.value.length > 12
        ? [
            {
              type: "slider",
              yAxisIndex: 0,
              right: 0,
              width: 12,
              startValue: 0,
              endValue: 11,
              filterMode: "filter",
              showDetail: false,
              brushSelect: false,
            },
            {
              type: "inside",
              yAxisIndex: 0,
              startValue: 0,
              endValue: 11,
              zoomOnMouseWheel: false,
              moveOnMouseWheel: true,
            },
          ]
        : [],
    grid: { bottom: 22, left: 8, right: 68, top: 14, containLabel: true },
    series: [
      {
        barMaxWidth: 28,
        data: countryData.value.map((country) => ({
          groupId: country.code,
          id: country.code,
          name: country.code,
          value: country.count,
          itemStyle: { color: countryColor(country.count, maxValue.value) },
        })),
        id: "country-pageviews",
        label: {
          color: colors.text,
          fontFamily: "Geist Mono Variable, monospace",
          formatter: "{c}",
          position: "right",
          show: true,
        },
        name: props.metricLabel,
        realtimeSort: false,
        type: "bar",
      },
    ],
    tooltip: {
      backgroundColor: colors.tooltipBg,
      borderColor: colors.grid,
      borderWidth: 1,
      textStyle: { color: colors.tooltipText },
      renderMode: "richText",
      formatter: (params: { name: string; value: unknown }) =>
        `${countryLabels.value[params.name] || countryLabel(params.name)}\n${Number.isFinite(Number(params.value)) ? Number(params.value).toLocaleString() + " " + props.metricLabel.toLowerCase() : "No recorded " + props.metricLabel.toLowerCase()}`,
      trigger: "item",
      valueFormatter: (value: unknown) => Number(value).toLocaleString(),
    },
    xAxis: {
      axisLabel: { color: colors.text },
      axisLine: { lineStyle: { color: colors.axis } },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: colors.grid } },
      type: "value",
    },
    yAxis: {
      axisLabel: {
        color: colors.text,
        formatter: (code: string) =>
          countryLabels.value[code] || countryLabel(code),
        fontWeight: 650,
        width: 150,
        overflow: "truncate",
      },
      axisLine: { show: false },
      axisTick: { show: false },
      data: countryData.value.map((country) => country.code),
      inverse: true,
      type: "category",
    },
  };
}

function normalizeCountryCode(value: string) {
  return toCountryCode(value, countryCodes.value);
}

function geoColors(host: HTMLElement) {
  return {
    axis: readCssVariable(host, "--chart-axis", "#8f887d"),
    grid: readCssVariable(host, "--chart-grid", "#d8d2c4"),
    text: readCssVariable(host, "--chart-text", "#6a6861"),
    tooltipBg: readCssVariable(host, "--chart-tooltip-bg", "#111214"),
    tooltipText: readCssVariable(host, "--chart-tooltip-text", "#f7f7f2"),
  };
}

function isAbortError(error: unknown) {
  return (
    typeof error === "object" &&
    error !== null &&
    "name" in error &&
    error.name === "AbortError"
  );
}
</script>

<template>
  <div class="geo-chart-shell">
    <div class="geo-toolbar">
      <div class="view-switcher" aria-label="Country chart view">
        <button
          type="button"
          :aria-pressed="view === 'bars'"
          :class="{ active: view === 'bars' }"
          @click="setView('bars')"
        >
          Ranked
        </button>
        <button
          type="button"
          :aria-pressed="view === 'globe'"
          :class="{ active: view === 'globe' }"
          :disabled="globeLoading"
          @click="setView('globe')"
        >
          {{ globeLoading ? "Loading globe…" : "Globe" }}
        </button>
      </div>
      <span v-if="globeError" class="geo-note">{{ globeError }}</span>
      <span v-else-if="globeLoading" class="geo-note">Preparing globe…</span>
      <span
        v-else-if="view === 'globe' && !hasCountries && !loading && !error"
        class="geo-note"
        >No country traffic in this range</span
      >
      <span v-else-if="!globeReady" class="geo-note"
        >{{ metricLabel }} by country</span
      >
      <span v-else class="geo-note">{{ metricLabel }} · darker means more</span>
    </div>

    <div class="chart-frame geo-chart-frame">
      <div v-if="loading && !hasCountries" class="chart-state" role="status">
        <span class="state-orbit" aria-hidden="true"></span>
        <strong>Loading country traffic</strong>
        <span>Reading country totals from the API.</span>
      </div>

      <div v-else-if="error" class="chart-state error" role="alert">
        <span class="state-symbol" aria-hidden="true">!</span>
        <strong>Country traffic is unavailable</strong>
        <span>{{ error }}</span>
        <button class="text-button" type="button" @click="emit('retry')">
          Try again
        </button>
      </div>

      <div
        v-else-if="!hasCountries && view === 'bars'"
        class="chart-state"
        role="status"
      >
        <span class="state-symbol" aria-hidden="true">◎</span>
        <strong>No country traffic in this range</strong>
        <span
          >Country totals appear as page views with country data arrive.</span
        >
      </div>

      <div
        v-else
        ref="chartEl"
        class="chart-surface geo-chart-surface"
        :class="{ 'geo-chart-hidden': view === 'globe' }"
        role="img"
        :aria-busy="loading"
        :aria-label="chartLabel"
      ></div>

      <Transition name="globe-fade">
        <LazyCountryGlobe
          v-if="
            view === 'globe' && geometry && !error && (!loading || hasCountries)
          "
          :metric-label="metricLabel"
          :geometry="geometry"
          :countries="
            countryData.map((country) => ({
              name: country.code,
              count: country.count,
            }))
          "
          :active="view === 'globe'"
        />
      </Transition>
      <p v-if="loading && hasCountries" class="sr-only" role="status">
        Updating country traffic for the selected date range.
      </p>

      <table v-if="hasCountries && !loading && !error" class="sr-only">
        <caption>
          Country
          {{
            metricLabel.toLowerCase()
          }}
          totals
        </caption>
        <thead>
          <tr>
            <th scope="col">Country</th>
            <th scope="col">{{ metricLabel }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="country in countryData" :key="country.code">
            <th scope="row">{{ country.label }}</th>
            <td>{{ country.count }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="geo-footer">
      <span
        >{{
          countryData.filter((country) => country.code !== "UNKNOWN").length
        }}
        countries · {{ metricLabel.toLowerCase()
        }}<span
          v-if="countryData.some((country) => country.code === 'UNKNOWN')"
        >
          · Unknown locations remain in the ranking</span
        ></span
      >
      <a
        v-if="globeReady"
        href="https://www.amcharts.com/"
        target="_blank"
        rel="noopener noreferrer"
        >Map data: amCharts</a
      >
    </div>
  </div>
</template>
<style scoped>
.geo-chart-frame {
  position: relative;
}
.geo-chart-hidden {
  opacity: 0;
  pointer-events: none;
  visibility: hidden;
}
.geo-chart-surface {
  transition: opacity 250ms ease;
}
.geo-footer {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 8px;
  font-size: 0.72rem;
  color: var(--text-tertiary);
}
.geo-footer a {
  color: inherit;
}
.globe-fade-enter-active,
.globe-fade-leave-active {
  transition: opacity 250ms ease;
}
.globe-fade-enter-from,
.globe-fade-leave-to {
  opacity: 0;
}
@media (prefers-reduced-motion: reduce) {
  .geo-chart-surface,
  .globe-fade-enter-active,
  .globe-fade-leave-active {
    transition: none;
  }
}
</style>
