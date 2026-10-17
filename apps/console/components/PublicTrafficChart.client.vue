<script setup lang="ts">
import type { PublicOverview, PublicMetric } from "~/types/publicDashboard";

defineProps<{
  points: NonNullable<PublicOverview["data"]["traffic"]>;
  metrics: PublicMetric[];
  rangeKey?: string;
  asOf?: string;
}>();
</script>

<template>
  <OverviewChart
    :points="points"
    :metrics="metrics"
    :as-of="asOf"
    :range-key="rangeKey"
    interval="day"
  />
</template>
