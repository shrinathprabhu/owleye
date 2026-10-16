<script setup lang="ts">
import type { StatsOverviewResponse } from "~/types/stats";
import { populationDescription } from "~/utils/reportPopulation";
import {
  breakdownItems,
  breakdownLabel,
  type BreakdownMetric,
} from "~/utils/breakdownMetric";
const props = defineProps<{
  stats?: StatsOverviewResponse | null;
  metric: BreakdownMetric;
  loading?: boolean;
  error?: string;
}>();
const tabs = [
  { key: "channels", label: "Channels" },
  { key: "referrers", label: "Referrers" },
  { key: "utm_sources", label: "UTM sources" },
  { key: "utm_mediums", label: "UTM mediums" },
  { key: "utm_campaigns", label: "UTM campaigns" },
] as const;
const selected = ref<(typeof tabs)[number]["key"]>("channels");
const items = computed(() =>
  breakdownItems(props.stats?.[selected.value], props.metric),
);
const maximum = computed(() =>
  Math.max(1, ...items.value.map((item) => item.count)),
);
const failed = computed(
  () =>
    props.stats?.measurement?.acquisition_status === "query_failed" &&
    ["channels", "utm_sources", "utm_mediums"].includes(selected.value),
);
const available = computed(
  () => !failed.value && props.stats?.[selected.value] !== undefined,
);
const title = computed(
  () => tabs.find((tab) => tab.key === selected.value)!.label,
);
</script>

<template>
  <article class="panel wide acquisition-summary">
    <div class="panel-header">
      <div>
        <p class="panel-kicker">Acquisition</p>
        <h2>Traffic sources</h2>
      </div>
      <span>{{ breakdownLabel(metric) }}</span>
    </div>
    <div
      class="acquisition-tabs"
      role="group"
      aria-label="Acquisition breakdown"
    >
      <button
        v-for="tab in tabs"
        :key="tab.key"
        type="button"
        :aria-pressed="selected === tab.key"
        @click="selected = tab.key"
      >
        {{ tab.label }}
      </button>
    </div>
    <p class="acquisition-note">
      {{
        loading && !stats
          ? "Loading acquisition policy…"
          : populationDescription(
              ["channels", "utm_sources", "utm_mediums"].includes(selected)
                ? stats?.measurement?.new_metrics_policy
                : stats?.measurement?.legacy_totals_policy,
            )
      }}
      Channels, UTM sources and UTM mediums use the new policy; referrers and
      campaigns preserve the legacy traffic population, so their totals can
      differ.
    </p>
    <p class="acquisition-note">
      {{
        metric === "visitors"
          ? "Pageview audiences can overlap across sources; do not sum them as people."
          : "Pageviews are grouped once within each selected breakdown."
      }}
      Campaign labels are supplied values; they do not establish paid
      advertising or causation.
    </p>
    <p
      v-if="selected === 'utm_sources' || selected === 'utm_mediums'"
      class="acquisition-note"
    >
      Only recognized marketing codes are shown. Unrecognized labels are grouped
      into Other or withheld; an empty list does not establish zero activity.
    </p>
    <p v-if="loading && !stats" class="empty-state" role="status">
      Loading traffic sources…
    </p>
    <p v-else-if="failed" class="empty-state" role="status">
      This acquisition breakdown could not be loaded. Refresh analytics to
      retry.
    </p>
    <p v-else-if="!available" class="empty-state">
      {{ title }} are unavailable from this report.
    </p>
    <div v-else-if="items.length" class="acquisition-rows" :aria-label="title">
      <div v-for="item in items" :key="item.name" class="acquisition-row">
        <div
          class="acquisition-bar"
          :style="{
            width: `${Math.max(0, Math.min(100, (item.count / maximum) * 100))}%`,
          }"
          aria-hidden="true"
        ></div>
        <span>{{
          item.name === "Direct" || !item.name ? "Direct / unknown" : item.name
        }}</span
        ><strong>{{ item.count.toLocaleString() }}</strong>
      </div>
    </div>
    <p v-else class="empty-state">
      No releasable {{ title.toLowerCase() }} in this range.
    </p>
    <p v-if="error && stats" class="acquisition-note" role="status">
      Last loaded sources · refresh failed.
    </p>
    <p v-if="stats?.measurement" class="acquisition-note">
      {{ stats.measurement.channel_version }} · Small groups may be withheld;
      Other is deduplicated from its eligible tail.
    </p>
  </article>
</template>

<style scoped>
.acquisition-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 16px 0;
}
.acquisition-tabs button {
  padding: 9px 13px;
  border: 1px solid var(--border-default);
  border-radius: 999px;
  color: var(--text-secondary);
  background: var(--bg-elevated);
  font: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}
.acquisition-tabs button[aria-pressed="true"] {
  color: var(--brand-text-safe);
  border-color: var(--brand-text-safe);
}
.acquisition-tabs button:focus-visible {
  outline: 3px solid var(--accent-primary, #4057ff);
  outline-offset: 3px;
}
.acquisition-note {
  color: var(--text-secondary);
  font-size: 0.78rem;
  line-height: 1.6;
}
.acquisition-rows {
  display: grid;
  gap: 7px;
}
.acquisition-row {
  display: flex;
  position: relative;
  isolation: isolate;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  min-height: 42px;
  padding: 9px 12px;
  overflow: hidden;
  border-radius: 8px;
}
.acquisition-bar {
  position: absolute;
  z-index: -1;
  inset: 0 auto 0 0;
  background: var(--accent-primary, #4057ff);
  opacity: 0.12;
  border-radius: 8px;
}
.acquisition-row > span {
  min-width: 0;
  overflow-wrap: anywhere;
}
.acquisition-row strong {
  font-variant-numeric: tabular-nums;
}
</style>
