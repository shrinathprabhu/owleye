<script setup lang="ts">
useHead({ title: "Campaigns · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Create privacy-bounded UTM links and follow attributed visitors through conversion."
    eyebrow="Acquisition"
    permission="analytics_read"
    title="Campaigns"
    v-slot="{ demo, site }"
  >
    <CampaignManager
      :can-write="site?.role === 'owner' || site?.role === 'admin'"
      :demo="demo"
      :site-id="site?.tracking_id"
    />
  </ConsoleSectionShell>
</template>
