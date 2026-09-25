<script setup lang="ts">
useHead({ title: "Events · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Filter, group, and sort the event stream without changing a single row."
    eyebrow="Read-only warehouse"
    permission="analytics_read"
    title="Events"
    v-slot="{ site }"
  >
    <EventExplorer :site-id="site?.id" />
  </ConsoleSectionShell>
</template>
