<script setup lang="ts">
useHead({ title: "Web Vitals · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Measure real-user LCP, INP, CLS, FCP, and TTFB at p50, p75, and p95."
    eyebrow="Real-user measurements"
    permission="analytics_read"
    title="Web Vitals"
    v-slot="{ site, apiBase }"
  >
    <PerformanceSetup v-if="site" :site-id="site.tracking_id" :api-base="apiBase" />
    <PerformanceExplorer :site-id="site?.tracking_id" />
  </ConsoleSectionShell>
</template>
