<script setup lang="ts">
import type { DimensionStat } from "~/types/stats";
import {
  countryColor,
  countryLabel,
  type CountryGeometry,
} from "~/utils/countryGeo";
import {
  normalizeCountryCode,
  COUNTRY_CODES_BY_NAME,
} from "~/utils/countryMap";

const props = withDefaults(
  defineProps<{
    countries?: DimensionStat[];
    emptyMessage?: string;
    error?: string;
    loading?: boolean;
    metricLabel?: string;
  }>(),
  {
    countries: () => [],
    emptyMessage:
      "Country totals appear as page views with country data arrive.",
    error: "",
    loading: false,
    metricLabel: "Page views",
  },
);
const emit = defineEmits<{ retry: [] }>();
const geometry = shallowRef<CountryGeometry | null>(null);
const showGlobe = ref(false);
const globeLoading = ref(false);
const globeError = ref("");
const selectedCountry = ref("");
const focusCountry = ref("");
const countryData = computed(() =>
  props.countries
    .filter((country) => country.count > 0)
    .map((country) => {
      const code = normalizeCountryCode(
        country.name.trim(),
        COUNTRY_CODES_BY_NAME,
      );
      return {
        code,
        name: code,
        count: country.count,
        label: code === "OTHER" ? "Other" : countryLabel(code),
      };
    })
    .sort((a, b) => b.count - a.count),
);
const hasCountries = computed(() => countryData.value.length > 0);
const maximum = computed(() =>
  Math.max(1, ...countryData.value.map((country) => country.count)),
);
const selected = computed(() =>
  countryData.value.find((country) => country.code === selectedCountry.value),
);
let controller: AbortController | undefined;
let media: MediaQueryList | undefined;
let disposed = false;

// The map is a static asset, loaded once per mounted report; selecting a country never requests data.
async function loadGlobe() {
  if (geometry.value || globeLoading.value || disposed) return;
  globeError.value = "";
  globeLoading.value = true;
  controller = new AbortController();
  try {
    const response = await fetch("/maps/world-countries.json", {
      signal: controller.signal,
    });
    if (!response.ok) throw new Error("Map unavailable");
    const data = (await response.json()) as CountryGeometry;
    if (!disposed) geometry.value = data;
  } catch (error) {
    if (
      !disposed &&
      !(error instanceof DOMException && error.name === "AbortError")
    )
      globeError.value =
        "The globe could not load. Country rankings are still available.";
  } finally {
    if (!disposed) globeLoading.value = false;
  }
}
function chooseCountry(code: string) {
  selectedCountry.value = code;
  focusCountry.value = code;
}
function updateViewport() {
  if (media?.matches && hasCountries.value) {
    showGlobe.value = true;
    void loadGlobe();
  }
}
function toggleGlobe() {
  showGlobe.value = !showGlobe.value;
  if (showGlobe.value) void loadGlobe();
}
watch(countryData, () => {
  if (
    !countryData.value.some((country) => country.code === selectedCountry.value)
  ) {
    selectedCountry.value = "";
    focusCountry.value = "";
  }
  updateViewport();
});
onMounted(() => {
  media = matchMedia("(min-width: 760px)");
  media.addEventListener("change", updateViewport);
  updateViewport();
});
onBeforeUnmount(() => {
  disposed = true;
  controller?.abort();
  media?.removeEventListener("change", updateViewport);
});
</script>

<template>
  <div class="geo-chart-shell" :aria-busy="loading">
    <div class="geo-toolbar">
      <span class="geo-note">{{ metricLabel }} by country</span>
      <button
        v-if="hasCountries && !error"
        class="text-button"
        type="button"
        :aria-expanded="showGlobe"
        @click="toggleGlobe"
      >
        {{ showGlobe ? "Hide globe" : "Show globe" }}
      </button>
    </div>
    <div v-if="loading && !hasCountries" class="chart-state" role="status">
      <span class="state-orbit" aria-hidden="true"></span
      ><strong>Loading country traffic</strong
      ><span>Reading country totals from the API.</span>
    </div>
    <div v-else-if="error" class="chart-state error" role="alert">
      <strong>Country traffic is unavailable</strong><span>{{ error }}</span
      ><button class="text-button" type="button" @click="emit('retry')">
        Try again
      </button>
    </div>
    <div v-else-if="!hasCountries" class="chart-state" role="status">
      <strong>No country traffic in this range</strong
      ><span>{{ emptyMessage }}</span>
    </div>
    <div v-else class="country-comparison" :class="{ 'with-globe': showGlobe }">
      <section
        v-if="showGlobe"
        class="country-map"
        aria-label="Interactive country globe"
      >
        <LazyCountryGlobe
          v-if="geometry"
          :geometry="geometry"
          :countries="countryData"
          :active="showGlobe"
          :metric-label="metricLabel"
          :selected-country="selectedCountry"
          :focus-country="focusCountry"
          @select="selectedCountry = $event"
        />
        <div v-else class="chart-state" role="status">
          <span
            v-if="globeLoading"
            class="state-orbit"
            aria-hidden="true"
          ></span
          ><span>{{ globeError || "Preparing globe…" }}</span
          ><button
            v-if="globeError"
            type="button"
            class="text-button"
            @click="loadGlobe"
          >
            Retry globe
          </button>
        </div>
      </section>
      <div class="country-ranking">
        <p class="ranking-help">
          Hover, focus or tap a country to highlight it.
        </p>
        <ul aria-label="Country ranking" class="country-list">
          <li v-for="country in countryData" :key="country.code">
            <button
              type="button"
              class="country-row"
              :class="{ selected: selectedCountry === country.code }"
              :aria-pressed="selectedCountry === country.code"
              @pointerenter="chooseCountry(country.code)"
              @focus="chooseCountry(country.code)"
              @click="chooseCountry(country.code)"
            >
              <span class="country-name">{{ country.label }}</span
              ><strong>{{ country.count.toLocaleString() }}</strong>
              <span class="country-track" aria-hidden="true"
                ><span
                  :style="{
                    width: `${(country.count / maximum) * 100}%`,
                    background: countryColor(country.count, maximum),
                  }"
                ></span
              ></span>
            </button>
          </li>
        </ul>
        <p class="country-selection" role="status">
          {{
            selected
              ? `${selected.label}: ${selected.count.toLocaleString()} ${metricLabel.toLowerCase()}${selected.code === "UNKNOWN" ? " · location unavailable" : ""}`
              : "Country totals stay visible while you explore."
          }}
        </p>
      </div>
    </div>
    <p v-if="loading && hasCountries" class="sr-only" role="status">
      Updating country traffic for the selected date range.
    </p>
    <div v-if="hasCountries && !error" class="geo-footer">
      <span
        >{{
          countryData.filter((country) => /^[A-Z]{2}$/.test(country.code))
            .length
        }}
        countries · {{ metricLabel.toLowerCase()
        }}<span
          v-if="countryData.some((country) => country.code === 'UNKNOWN')"
        >
          · Unknown locations remain in the ranking</span
        ></span
      ><a
        v-if="geometry && showGlobe"
        href="https://www.amcharts.com/"
        target="_blank"
        rel="noopener noreferrer"
        >Map data: amCharts</a
      >
    </div>
  </div>
</template>

<style scoped>
.geo-chart-shell {
  display: grid;
  gap: 12px;
  min-width: 0;
}
.geo-toolbar,
.geo-footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.geo-note,
.geo-footer,
.ranking-help,
.country-selection {
  color: var(--text-secondary);
  font-size: 0.75rem;
}
.geo-footer a {
  color: inherit;
}
.country-comparison {
  display: grid;
  gap: 20px;
  min-width: 0;
}
.country-comparison.with-globe {
  grid-template-columns: minmax(0, 1fr) minmax(240px, 1fr);
}
.country-map {
  position: relative;
  min-width: 0;
  min-height: 370px;
}
.country-ranking {
  min-width: 0;
}
.ranking-help {
  margin: 0 0 10px;
}
.country-list {
  display: grid;
  gap: 5px;
  max-height: 430px;
  overflow: auto;
  padding: 4px;
  margin: 0;
  list-style: none;
  scrollbar-gutter: stable;
}
.country-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 7px 14px;
  width: 100%;
  min-height: 56px;
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  color: var(--text-primary);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.country-row.selected {
  background: var(--bg-accent-subtle);
  border-color: var(--accent-primary, #4057ff);
}
.country-row:focus-visible {
  outline: 2px solid var(--accent-primary, #4057ff);
  outline-offset: 1px;
}
.country-name {
  overflow-wrap: anywhere;
  font-size: 0.86rem;
}
.country-row strong {
  font:
    650 0.85rem "Geist Mono Variable",
    monospace;
}
.country-track {
  grid-column: 1 / -1;
  height: 5px;
  border-radius: 5px;
  overflow: hidden;
  background: var(--border-subtle);
}
.country-track > span {
  display: block;
  min-width: 2px;
  height: 100%;
  border-radius: inherit;
}
.country-selection {
  min-height: 2.5em;
  margin: 10px 0 0;
}
@media (max-width: 759px) {
  .country-comparison.with-globe {
    grid-template-columns: minmax(0, 1fr);
  }
  .country-ranking {
    order: -1;
  }
  .country-map {
    min-height: 330px;
  }
  .country-list {
    max-height: 350px;
  }
}
</style>
