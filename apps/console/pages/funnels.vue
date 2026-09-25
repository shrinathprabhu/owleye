<script setup lang="ts">
useHead({ title: "Funnels · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Create ordered conversion journeys and measure cohort drop-off with real analytics."
    eyebrow="Conversion analysis"
    permission="analytics_read"
    title="Funnels"
    v-slot="{ demo, site }"
  >
    <FunnelWorkspace :demo="demo" :site="site" />
  </ConsoleSectionShell>
</template>
