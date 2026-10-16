<script setup lang="ts">
import type {
  CohortPoint,
  CohortSummary,
  MeasurementContext,
} from "~/types/stats";
import {
  cohortMetrics,
  cohortMetricText,
  cohortSparkline,
  cohortScaleMaximum,
  cohortValue,
  type CohortMetric,
} from "~/utils/cohortPresentation";
import { toggleChartMetric } from "~/utils/chartSelection";
import { populationDescription } from "~/utils/reportPopulation";
const props = defineProps<{
  cohorts?: CohortSummary;
  points?: CohortPoint[];
  measurement?: MeasurementContext;
  entryPages?: { path: string; cohorts: number }[];
  lastPages?: { path: string; cohorts: number }[];
  loading?: boolean;
  error?: string;
}>();
const metricKeys: CohortMetric[] = ["cohorts", "bounce", "span"];
const selected = ref<CohortMetric[]>([...metricKeys]);
function toggle(key: string) {
  selected.value = toggleChartMetric(
    selected.value,
    key as CohortMetric,
    metricKeys,
  );
}
const colors: Record<CohortMetric, string> = {
  cohorts: "var(--owleye-lime)",
  bounce: "var(--owleye-violet)",
  span: "var(--owleye-coral)",
};
const units: Record<CohortMetric, string> = {
  cohorts: "Cohorts per bucket",
  bounce: "Single-page cohort percentage",
  span: "Mean first-to-last span",
};
const showTable = ref(false);
const chartId = useId();
const rows = computed(() =>
  props.cohorts?.status === "unavailable_in_demo" ||
  props.cohorts?.status === "query_failed"
    ? []
    : [...(props.points ?? [])].sort((a, b) => a.date.localeCompare(b.date)),
);
const cards = computed(() =>
  cohortMetrics.map((card) => ({
    ...card,
    value: cohortValue(props.cohorts, card.key),
    spark: cohortSparkline(rows.value, card.key),
    maximum: cohortScaleMaximum(rows.value, card.key),
  })),
);
const selectedCards = computed(() =>
  cards.value.filter((card) => selected.value.includes(card.key)),
);
const message = computed(() =>
  props.loading && !props.cohorts
    ? "Loading cohort measurements…"
    : props.cohorts?.status === "query_failed"
      ? "Cohort measurements could not be loaded. Refresh analytics to retry; other report totals remain available."
      : props.cohorts?.status === "unavailable_in_demo"
        ? "These measurements are unavailable in the static demo. Connect a site to measure eligible cohorts."
        : !props.cohorts
          ? "Cohort measurements are unavailable from this report."
          : props.cohorts.eligible_cohorts === 0
            ? "No eligible closed pageview cohorts in this range. Bounce and span remain unavailable."
            : undefined,
);
</script>

<template>
  <article class="panel wide cohort-overview" :aria-busy="loading">
    <div class="panel-header">
      <div>
        <p class="panel-kicker">Observed engagement</p>
        <h2>Sessions &amp; engagement</h2>
      </div>
      <span>Fixed cohorts · UTC</span>
    </div>
    <p class="cohort-context">
      These estimates use completed, eligible 30-minute pageview cohorts.
      Single-page visits can still be successful.
      {{
        loading && !cohorts
          ? "Loading population policy…"
          : populationDescription(measurement?.new_metrics_policy)
      }}
      These cohorts have a different population from the legacy traffic totals.
    </p>
    <div
      class="cohort-cards"
      role="group"
      aria-label="Select a card to view only that cohort trend"
    >
      <button
        v-for="card in cards"
        :key="card.key"
        type="button"
        class="console-selectable-card"
        :style="{ '--cohort-color': colors[card.key] }"
        :class="{ 'is-selected': selected.includes(card.key) }"
        :aria-pressed="selected.includes(card.key)"
        :aria-controls="chartId"
        @click="selected = [card.key]"
      >
        <span>{{ card.label }}</span
        ><strong>{{ cohortMetricText(card.value, card.key) }}</strong>
        <svg
          v-if="card.spark.path && !error"
          class="chart-sparkline"
          viewBox="0 0 240 60"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <path :d="card.spark.path" />
        </svg>
        <span v-else class="cohort-no-spark">{{
          loading && !cohorts ? "Loading…" : "No available trend"
        }}</span>
        <small>{{ card.note }}</small>
      </button>
    </div>
    <ChartSeriesControls
      :options="cards"
      :selected="selected"
      :chart-id="chartId"
      @toggle="toggle"
      @all="selected = [...metricKeys]"
    />
    <p
      v-if="message"
      class="empty-state"
      :class="{ 'cohort-loading-frame': loading && !cohorts }"
      role="status"
    >
      {{ message }}
    </p>
    <section
      v-else
      :id="chartId"
      class="panel data-stage cohort-trend"
      aria-label="Selected cohort trends"
    >
      <p class="cohort-scale-note">
        Each measurement has its own labelled scale. Gaps are unknown
        measurements, not zero.
      </p>
      <div
        class="cohort-plots"
        :style="{ '--plot-count': selectedCards.length }"
      >
        <section
          v-for="card in selectedCards"
          :key="card.key"
          class="cohort-plot"
          :style="{ '--cohort-color': colors[card.key] }"
          :aria-label="`${card.label} trend`"
        >
          <h3>{{ card.label }}</h3>
          <p class="cohort-unit">{{ units[card.key] }}</p>
          <div v-if="card.spark.path" class="cohort-plot-body">
            <div class="cohort-y-axis" aria-hidden="true">
              <span>{{ cohortMetricText(card.maximum, card.key) }}</span
              ><span>{{ cohortMetricText(0, card.key) }}</span>
            </div>
            <svg
              viewBox="0 0 240 60"
              preserveAspectRatio="none"
              role="img"
              :aria-label="`${card.label}; scale zero to ${cohortMetricText(card.maximum, card.key)}. UTC bucket values and sample counts are in the table below.`"
            >
              <g class="cohort-grid" aria-hidden="true">
                <line x1="0" y1="8" x2="240" y2="8" />
                <line x1="0" y1="30" x2="240" y2="30" />
                <line x1="0" y1="52" x2="240" y2="52" />
              </g>
              <path
                v-if="card.spark.area"
                class="cohort-area"
                :d="card.spark.area"
                aria-hidden="true"
              />
              <path :d="card.spark.path" />
              <circle
                v-if="card.spark.point"
                :cx="card.spark.point[0]"
                :cy="card.spark.point[1]"
                r="2"
              />
            </svg>
          </div>
          <p v-else class="cohort-no-measurements">
            No eligible samples. Missing measurements are not zero.
          </p>
          <div v-if="rows.length" class="cohort-trend-dates">
            <span>{{ rows[0]?.date }}</span
            ><span>{{ rows.at(-1)?.date }}</span>
          </div>
        </section>
      </div>
      <details @toggle="showTable = ($event.target as HTMLDetailsElement).open">
        <summary>View selected bucket values &amp; sample counts</summary>
        <div v-if="showTable" class="cohort-table-scroll">
          <table>
            <caption>
              Selected measurements · UTC buckets · independent units
            </caption>
            <thead>
              <tr>
                <th>Bucket</th>
                <template v-for="card in selectedCards" :key="card.key"
                  ><th>{{ card.label }}</th>
                  <th>
                    {{
                      card.key === "span" ? "Span samples" : "Eligible cohorts"
                    }}
                  </th></template
                >
              </tr>
            </thead>
            <tbody>
              <tr v-for="point in rows" :key="point.date">
                <td>{{ point.date }}</td>
                <template v-for="card in selectedCards" :key="card.key"
                  ><td>
                    {{
                      cohortMetricText(cohortValue(point, card.key), card.key)
                    }}
                  </td>
                  <td>
                    {{
                      (card.key === "span"
                        ? point.observed_span_samples
                        : point.eligible_cohorts
                      ).toLocaleString()
                    }}
                  </td></template
                >
              </tr>
            </tbody>
          </table>
        </div>
      </details>
    </section>
    <p v-if="error && cohorts" class="cohort-context" role="status">
      Last loaded measurements · refresh failed.
    </p>
    <details
      v-if="
        cohorts?.status !== 'query_failed' &&
        (entryPages !== undefined || lastPages !== undefined)
      "
      class="cohort-definitions"
    >
      <summary>Entry &amp; last-observed pages</summary>
      <p>
        Pages use all available observations in each eligible cohort. A
        last-observed page does not establish that a visitor closed the site.
      </p>
      <div class="cohort-page-grid">
        <section
          v-for="group in [
            {
              label: 'Entry pages',
              rows: entryPages,
              withheld: cohorts?.entry_pages_withheld,
            },
            {
              label: 'Last-observed pages',
              rows: lastPages,
              withheld: cohorts?.last_pages_withheld,
            },
          ]"
          :key="group.label"
        >
          <h3>{{ group.label }}</h3>
          <div
            v-for="page in group.rows"
            :key="page.path"
            class="cohort-page-row"
          >
            <span>{{ page.path }}</span
            ><strong>{{ page.cohorts.toLocaleString() }}</strong>
          </div>
          <p v-if="!group.rows?.length">
            {{
              group.rows === undefined
                ? "Unavailable from this report."
                : "No releasable page groups in this range."
            }}
          </p>
          <p v-if="group.withheld">
            Some page groups are withheld for privacy.
          </p>
        </section>
      </div>
    </details>
    <details
      v-if="
        cohorts &&
        cohorts.status !== 'query_failed' &&
        cohorts.status !== 'unavailable_in_demo' &&
        measurement
      "
      class="cohort-definitions"
    >
      <summary>Definition &amp; coverage</summary>
      <p>
        {{ cohorts.eligible_cohorts.toLocaleString() }} eligible cohorts;
        {{ cohorts.excluded_open_or_boundary_cohorts.toLocaleString() }} open or
        boundary cohorts excluded.
        {{ cohorts.observed_span_samples.toLocaleString() }} cohorts have enough
        observations for span.
      </p>
      <p>
        Median observed span:
        {{
          cohortMetricText(
            cohorts.observed_span_samples > 0
              ? (cohorts.median_observed_span_ms ?? undefined)
              : undefined,
            "span",
          )
        }}.
      </p>
      <p>
        Page/custom/rule activity determines first-to-last span; one observation
        has unknown duration. Visible-page segment time in Summary remains a
        separate measurement. Cohorts close after a
        {{ measurement.delivery_grace_seconds / 60 }}-minute delivery grace.
        Late events can revise these results; retained rows do not prove
        historical completeness.
      </p>
      <p>
        Definition {{ measurement.definition_version }} · observed as of
        {{ measurement.as_of }} (UTC).
      </p>
    </details>
  </article>
</template>

<style scoped>
.cohort-context,
.cohort-definitions p {
  color: var(--text-secondary);
  font-size: 0.8rem;
  line-height: 1.6;
}
.cohort-cards {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin: 18px 0;
}
.cohort-cards button {
  display: grid;
  gap: 9px;
  text-align: left;
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--border-default);
  border-radius: 14px;
  background: var(--bg-elevated);
  color: var(--text-primary);
  font: inherit;
  cursor: pointer;
}
.cohort-cards button:focus-visible {
  outline: 3px solid var(--accent-primary, #4057ff);
  outline-offset: 3px;
}
.cohort-cards strong {
  font-size: clamp(1.5rem, 3vw, 2.3rem);
  font-variant-numeric: tabular-nums;
}
.cohort-cards small,
.cohort-no-spark {
  color: var(--text-secondary);
  font-size: 0.75rem;
}
.cohort-cards svg,
.cohort-no-spark {
  height: 48px;
  width: 100%;
}
.cohort-cards path {
  fill: none;
  stroke: var(--cohort-color, var(--accent-primary, #4057ff));
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
  stroke-linejoin: round;
  stroke-linecap: round;
}
.cohort-loading-frame {
  min-height: 290px;
}
.cohort-trend {
  padding: var(--chart-panel-padding);
  margin: 0;
}
.cohort-scale-note {
  color: var(--chart-text);
  font-size: 0.75rem;
  line-height: 1.6;
  margin: 0 0 18px;
}
.cohort-plots {
  display: grid;
  grid-template-columns: repeat(var(--plot-count), minmax(0, 1fr));
  gap: 24px;
}
.cohort-plot {
  min-width: 0;
}
.cohort-plot h3 {
  color: var(--cohort-color);
  font-size: 0.9rem;
  margin: 0 0 5px;
}
.cohort-unit {
  color: var(--chart-text);
  font-size: 0.7rem;
  margin: 0 0 8px;
}
.cohort-plot-body {
  position: relative;
  padding-left: 66px;
}
.cohort-y-axis {
  position: absolute;
  left: 0;
  top: 0;
  height: 130px;
  width: 59px;
  color: var(--chart-text);
  font-size: 0.7rem;
  font-variant-numeric: tabular-nums;
  text-align: right;
}
.cohort-y-axis span {
  position: absolute;
  right: 0;
  transform: translateY(-50%);
}
.cohort-y-axis span:first-child {
  top: 13.333%;
}
.cohort-y-axis span:last-child {
  top: 86.667%;
}
.cohort-plot-body svg {
  display: block;
  width: 100%;
  height: 130px;
  overflow: visible;
}
.cohort-grid line {
  stroke: var(--chart-grid);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
}
.cohort-trend path {
  fill: none;
  stroke: var(--cohort-color);
  stroke-width: 2.5;
  vector-effect: non-scaling-stroke;
  stroke-linejoin: round;
  stroke-linecap: round;
}
.cohort-trend .cohort-area {
  fill: var(--cohort-color);
  fill-opacity: 0.1;
  stroke: none;
}
.cohort-trend circle {
  fill: var(--cohort-color);
}
.cohort-no-measurements {
  min-height: 130px;
  color: var(--chart-text);
  font-size: 0.75rem;
  display: flex;
  align-items: center;
}
.cohort-trend-dates {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding-left: 66px;
  color: var(--chart-text);
  font-size: 0.65rem;
}
.cohort-trend summary {
  cursor: pointer;
  color: var(--owleye-lime);
  font-size: 0.8rem;
  margin-top: 22px;
}
.cohort-trend table {
  color: var(--chart-text);
}
.cohort-trend th,
.cohort-trend td,
.cohort-trend caption {
  border-color: var(--chart-grid);
}
.cohort-definitions summary {
  cursor: pointer;
  color: var(--brand-text-safe);
  font-size: 0.8rem;
  margin-top: 16px;
}
.cohort-table-scroll {
  max-width: 100%;
  overflow-x: auto;
}
.cohort-page-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 20px;
}
.cohort-page-grid h3 {
  font-size: 0.9rem;
}
.cohort-page-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 0;
  border-bottom: 1px solid var(--border-subtle);
  font-size: 0.8rem;
}
.cohort-page-row span {
  min-width: 0;
  overflow-wrap: anywhere;
}
.cohort-page-row strong {
  font-variant-numeric: tabular-nums;
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.8rem;
}
caption,
th,
td {
  padding: 9px;
  text-align: left;
  border-bottom: 1px solid var(--border-subtle);
}
@media (max-width: 700px) {
  .cohort-cards,
  .cohort-plots,
  .cohort-page-grid {
    grid-template-columns: 1fr;
  }
  .cohort-trend {
    padding: 18px 14px;
  }
  .cohort-loading-frame {
    min-height: 790px;
  }
}
</style>
