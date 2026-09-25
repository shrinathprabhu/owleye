<script setup lang="ts">
useHead({ title: "Live activity · OWLEYE" });
</script>
<template>
  <ConsoleSectionShell
    title="Live"
    eyebrow="Recent activity"
    description="Follow incoming traffic and events. Pause updates whenever you need."
    permission="analytics_read"
    v-slot="{ site, demo }"
  >
    <LiveAnalytics :site-id="site?.id" :demo="demo" />
  </ConsoleSectionShell>
</template>
