<script setup lang="ts">
useHead({ title: "Campaigns · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Create UTM links and compare attributed traffic and conversions."
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
