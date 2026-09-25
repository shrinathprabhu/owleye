<script setup lang="ts">
useHead({ title: "Uptime · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    title="Uptime"
    eyebrow="A watchful eye, every five minutes"
    description="Know when your public website or API stops responding, and when it comes back."
    v-slot="{ demo, site, permissions }"
  >
    <UptimeManager
      :site-id="site?.id"
      :demo="demo"
      :can-manage="permissions.rules_write"
    />
  </ConsoleSectionShell>
</template>
